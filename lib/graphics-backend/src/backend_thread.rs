use std::sync::{
    Arc,
    atomic::AtomicU64,
    mpsc::{Receiver, Sender, SyncSender},
};

use anyhow::anyhow;
use base::join_thread::JoinThread;
use base_io::io::IoFileSys;
use config::config::{ConfigBackend, ConfigDebug};
use graphics_backend_traits::{
    frame_fetcher_plugin::BackendFrameFetcher, traits::DriverBackendInterface,
};
use graphics_base_traits::traits::{
    GraphicsStreamUniformRawDataStatic, GraphicsStreamVertices, GraphicsStreamVerticesStatic,
    GraphicsStreamedData, GraphicsStreamedDataSyncSend, GraphicsStreamedUniformData,
    GraphicsStreamedUniformRawData,
};
use graphics_types::{
    commands::{
        AllCommands, GRAPHICS_DEFAULT_UNIFORM_SIZE, GRAPHICS_MAX_UNIFORM_RENDER_COUNT,
        StreamDataMax,
    },
    rendering::GlVertex,
};
use hiarc::Hiarc;
use pool::{mixed_pool::PoolSyncPoint, mt_datatypes::PoolVec};

use crate::{
    backend::CustomPipelines,
    backend_mt::GraphicsBackendMtType,
    backends::{
        null::NullBackend,
        types::BackendWriteFiles,
        vulkan::{
            Options,
            vulkan::{
                VulkanBackend, VulkanBackendLoadedIo, VulkanBackendLoading, VulkanInUseStreamData,
                VulkanMainThreadData, VulkanMainThreadInit,
            },
        },
    },
    cache::get_backend_cache,
    window::{BackendDisplayRequirements, BackendWindow},
};

#[derive(Debug)]
enum GraphicsBackendLoadingType {
    Vulkan(Box<VulkanBackendLoading>),
    Null(NullBackend),
}

#[derive(Debug, Hiarc)]
enum GraphicsBackendType {
    Vulkan(Box<VulkanBackend>),
    Null(NullBackend),
}

impl GraphicsBackendType {
    pub fn as_mut(&mut self) -> &mut dyn DriverBackendInterface {
        match self {
            GraphicsBackendType::Vulkan(backend) => backend.as_mut(),
            GraphicsBackendType::Null(backend) => backend,
        }
    }
}

#[derive(Debug, Hiarc)]
pub enum BackendThreadInitData {
    Vulkan {
        data: VulkanBackendLoadedIo,

        runtime_threadpool: Arc<rayon::ThreadPool>,

        window_width: u32,
        window_height: u32,
        dbg: ConfigDebug,
        gl: ConfigBackend,
    },
    Null,
}

#[derive(Debug, Hiarc)]
pub enum BackendThreadInitFromMainThread {
    Vulkan(VulkanMainThreadData),
    Null,
}

#[derive(Debug, Hiarc)]
pub enum BackendThreadMainThreadInit {
    Vulkan(VulkanMainThreadInit),
    Null,
}

#[derive(Debug, Hiarc)]
pub enum BackendThreadBackendEvent {
    Init {
        backend_ty: String,
        display_requirements: BackendDisplayRequirements,
        config_dbg: ConfigDebug,
        config_gl: ConfigBackend,
        #[hiarc_skip_unsafe]
        custom_pipes: Option<CustomPipelines>,
        texture_memory_usage: Arc<AtomicU64>,
        buffer_memory_usage: Arc<AtomicU64>,
        stream_memory_usage: Arc<AtomicU64>,
        staging_memory_usage: Arc<AtomicU64>,
        write_files: BackendWriteFiles,
    },
    FinishInit {
        data: BackendThreadInitData,
        main_thread_init: BackendThreadMainThreadInit,
    },
    RunCmds {
        cmds: Vec<AllCommands>,
        stream_data: GraphicsStreamedDataSyncSend,
    },
    AttachFrameFetcher {
        name: String,
        #[hiarc_skip_unsafe]
        fetcher: Arc<dyn BackendFrameFetcher>,
    },
    DetachFrameFetcher {
        name: String,
    },
    WindowCreateNtfy {
        oneshot: SyncSender<BackendThreadInitFromMainThread>,
    },
    WindowCreated {
        main_thread_init: BackendThreadMainThreadInit,
    },
    WindowDestroyNtfy(SyncSender<()>),
    WaitForIdle(SyncSender<()>),
    Stop,
}

#[derive(Debug, Hiarc)]
pub enum BackendThreadFrontendEvent {
    InitFromMainThread(BackendThreadInitFromMainThread),
    BuffersFromBackend {
        streamed_data: GraphicsStreamedDataSyncSend,
        /// empty cmd buffer, can be reused
        cmds: Vec<AllCommands>,
    },
}

#[derive(Debug, Hiarc)]
struct FileWriterDrop {
    write_files: BackendWriteFiles,
    io: IoFileSys,
}

impl Drop for FileWriterDrop {
    fn drop(&mut self) {
        for (path, write_file) in std::mem::take(&mut *self.write_files.lock()) {
            let fs = self.io.fs.clone();
            self.io.rt.spawn_without_lifetime(async move {
                let cache = get_backend_cache(&fs).await;
                cache.write_named(&path, write_file).await?;
                anyhow::Ok(())
            });
        }
    }
}

#[derive(Debug, Hiarc)]
pub struct BackendThread {
    events: Sender<BackendThreadBackendEvent>,
    recv_events: Receiver<BackendThreadFrontendEvent>,

    #[hiarc_skip_unsafe]
    sync_points: Vec<Box<dyn PoolSyncPoint>>,

    // custom drop, must stay second element
    _thread: JoinThread<anyhow::Result<()>>,
    // custom drop, must stay last element
    _file_writer: FileWriterDrop,
}

impl BackendThread {
    pub fn new(
        backend_ty: String,
        display_requirements: BackendDisplayRequirements,
        config_dbg: ConfigDebug,
        config_gl: ConfigBackend,
        custom_pipes: Option<CustomPipelines>,
        texture_memory_usage: Arc<AtomicU64>,
        buffer_memory_usage: Arc<AtomicU64>,
        stream_memory_usage: Arc<AtomicU64>,
        staging_memory_usage: Arc<AtomicU64>,
        io: IoFileSys,
    ) -> anyhow::Result<Self> {
        let (events, recv) = std::sync::mpsc::channel();
        let (sender, frontend_events) = std::sync::mpsc::channel();

        let write_files = BackendWriteFiles::default();

        events.send(BackendThreadBackendEvent::Init {
            backend_ty,
            display_requirements,
            config_dbg,
            config_gl,
            custom_pipes,
            texture_memory_usage,
            buffer_memory_usage,
            stream_memory_usage,
            staging_memory_usage,
            write_files: write_files.clone(),
        })?;

        let thread = thread_priority::ThreadBuilder::default()
            .name("backend-thread".to_string())
            .priority(thread_priority::ThreadPriority::Max)
            .spawn_careless(move || match BackendThread::run(recv, sender) {
                Ok(_) => Ok(()),
                Err(err) => {
                    log::info!("graphics backend thread exited: {err}");
                    Err(err)
                }
            })?;

        Ok(Self {
            events,
            recv_events: frontend_events,
            sync_points: Default::default(),
            _thread: JoinThread::new(thread),
            _file_writer: FileWriterDrop { write_files, io },
        })
    }

    pub fn init(
        &self,
        data: BackendThreadInitData,
        dbg: &ConfigDebug,
        window: &BackendWindow,
    ) -> anyhow::Result<GraphicsBackendMtType> {
        let BackendThreadFrontendEvent::InitFromMainThread(init_ev) = self.recv_events.recv()?
        else {
            return Err(anyhow!(
                "Frontend event was not sent from the backend thread, \
                maybe it died? else it's a logic bug"
            ));
        };
        let (main_thread_init, backend_mt) = match init_ev {
            BackendThreadInitFromMainThread::Vulkan(data) => {
                let backend_mt = VulkanBackend::create_mt_backend(&data);
                (
                    BackendThreadMainThreadInit::Vulkan(VulkanBackend::init_from_main_thread(
                        data, window, dbg,
                    )?),
                    GraphicsBackendMtType::Vulkan(backend_mt),
                )
            }
            BackendThreadInitFromMainThread::Null => (
                BackendThreadMainThreadInit::Null,
                GraphicsBackendMtType::Null(NullBackend::get_mt_backend()),
            ),
        };
        self.events.send(BackendThreadBackendEvent::FinishInit {
            data,
            main_thread_init,
        })?;

        Ok(backend_mt)
    }

    pub fn window_created_ntfy(
        &self,
        window: BackendWindow,
        dbg: &ConfigDebug,
    ) -> anyhow::Result<()> {
        let (sender, recv) = std::sync::mpsc::sync_channel(0);
        self.events
            .send(BackendThreadBackendEvent::WindowCreateNtfy { oneshot: sender })?;

        let init_ev = recv.recv()?;

        let main_thread_init = match init_ev {
            BackendThreadInitFromMainThread::Vulkan(data) => {
                VulkanBackend::init_from_main_thread(data, &window, dbg)
                    .map(BackendThreadMainThreadInit::Vulkan)
            }
            BackendThreadInitFromMainThread::Null => Ok(BackendThreadMainThreadInit::Null),
        }?;
        self.events
            .send(BackendThreadBackendEvent::WindowCreated { main_thread_init })?;
        Ok(())
    }

    pub fn window_destroyed_ntfy(&self) -> anyhow::Result<()> {
        let (sender, recv) = std::sync::mpsc::sync_channel(0);
        self.events
            .send(BackendThreadBackendEvent::WindowDestroyNtfy(sender))?;
        recv.recv()?;
        Ok(())
    }

    pub fn wait_idle(&self) -> anyhow::Result<()> {
        let (sender, recv) = std::sync::mpsc::sync_channel(0);
        self.events
            .send(BackendThreadBackendEvent::WaitForIdle(sender))?;
        recv.recv()?;
        Ok(())
    }

    /// add a pool sync pointer before the [`BackendThread::run_cmds`] command is called
    /// sync points can not be removed, so call carefully
    pub fn add_sync_point(&mut self, sync_point: Box<dyn PoolSyncPoint>) {
        self.sync_points.push(sync_point);
    }

    pub fn run_cmds(
        &self,
        stream_data: &GraphicsStreamedData,
        cmds: &mut Vec<AllCommands>,
    ) -> anyhow::Result<()> {
        let BackendThreadFrontendEvent::BuffersFromBackend {
            streamed_data: stream_data_cmd,
            cmds: mut cmds_cmd,
        } = self.recv_events.recv()?
        else {
            return Err(anyhow!(
                "frontend commands other than stream data is not supported yet, also there must be a stream data command every frame"
            ));
        };

        std::mem::swap(cmds, &mut cmds_cmd);

        let stream_data_cmd = GraphicsStreamedData::from_sync_send_wrapper(stream_data_cmd);
        let stream_data = stream_data
            .try_replace_inner(stream_data_cmd)
            .map_err(|err| anyhow!(err))?;

        for sync_point in &self.sync_points {
            sync_point.sync();
        }

        self.events.send(BackendThreadBackendEvent::RunCmds {
            cmds: cmds_cmd,
            stream_data: stream_data.try_into_sync_send_wrapper()?,
        })?;
        Ok(())
    }

    pub fn attach_frame_fetcher(
        &mut self,
        name: String,
        fetcher: Arc<dyn BackendFrameFetcher>,
    ) -> anyhow::Result<()> {
        self.events
            .send(BackendThreadBackendEvent::AttachFrameFetcher { name, fetcher })?;
        Ok(())
    }

    pub fn detach_frame_fetcher(&mut self, name: String) -> anyhow::Result<()> {
        self.events
            .send(BackendThreadBackendEvent::DetachFrameFetcher { name })?;
        Ok(())
    }

    fn run(
        events: Receiver<BackendThreadBackendEvent>,
        sender: Sender<BackendThreadFrontendEvent>,
    ) -> anyhow::Result<()> {
        // handle loading
        let load_ev = events.recv()?;
        let BackendThreadBackendEvent::Init {
            backend_ty,
            display_requirements,
            config_dbg,
            config_gl,
            custom_pipes,
            texture_memory_usage,
            buffer_memory_usage,
            stream_memory_usage,
            staging_memory_usage,
            write_files,
        } = load_ev
        else {
            return Err(anyhow!("first event is always the load event"));
        };
        let backend_loading = match backend_ty.to_ascii_lowercase().as_str() {
            "null" => GraphicsBackendLoadingType::Null(NullBackend {}),
            // "vulkan"
            _ => {
                let options = Options {
                    dbg: &config_dbg,
                    gl: &config_gl,
                };
                // prepare the GL instance
                let backend = VulkanBackendLoading::new(
                    display_requirements,
                    texture_memory_usage,
                    buffer_memory_usage,
                    stream_memory_usage,
                    staging_memory_usage,
                    &options,
                    custom_pipes,
                )?;
                GraphicsBackendLoadingType::Vulkan(Box::new(backend))
            }
        };

        sender.send(BackendThreadFrontendEvent::InitFromMainThread(
            match &backend_loading {
                GraphicsBackendLoadingType::Vulkan(loading) => {
                    BackendThreadInitFromMainThread::Vulkan(VulkanBackend::main_thread_data(
                        loading,
                    ))
                }
                GraphicsBackendLoadingType::Null(_) => BackendThreadInitFromMainThread::Null,
            },
        ))?;

        let load_ev = events.recv()?;
        let BackendThreadBackendEvent::FinishInit {
            data,
            main_thread_init,
        } = load_ev
        else {
            return Err(anyhow!(
                "finish init event is always the second event, found: {:?}",
                load_ev
            ));
        };

        let mut backend = match data {
            BackendThreadInitData::Vulkan {
                data,
                runtime_threadpool,
                window_width,
                window_height,
                dbg,
                gl,
            } => {
                let GraphicsBackendLoadingType::Vulkan(loading) = backend_loading else {
                    return Err(anyhow!("loading was not of type vulkan"));
                };
                let BackendThreadMainThreadInit::Vulkan(main_thread_init) = main_thread_init else {
                    return Err(anyhow!("main thread init data was not of type vulkan"));
                };
                GraphicsBackendType::Vulkan(VulkanBackend::new(
                    *loading,
                    data,
                    &runtime_threadpool,
                    main_thread_init,
                    window_width,
                    window_height,
                    &Options { dbg: &dbg, gl: &gl },
                    write_files,
                )?)
            }
            BackendThreadInitData::Null => GraphicsBackendType::Null(NullBackend {}),
        };

        enum InUseDataPerBackend {
            Vulkan(VulkanInUseStreamData),
            Null,
        }

        let (mut stream_data, mut next_in_use_data, mut in_use_data) = match &mut backend {
            GraphicsBackendType::Vulkan(backend) => {
                let stream_data = backend.get_stream_data()?;
                let next_stream_data = backend.get_stream_data()?;

                let mem = unsafe {
                    stream_data.cur_stream_vertex_buffer.memories[0]
                        .mapped_memory
                        .get_mem_typed::<GlVertex>(StreamDataMax::MaxVertices as usize)
                };

                let mut graphics_uniform_data = backend.props.graphics_uniform_buffers.new();
                graphics_uniform_data.extend(
                    stream_data
                        .cur_stream_uniform_buffers
                        .memories
                        .iter()
                        .map(|uni| unsafe {
                            GraphicsStreamedUniformData::new(GraphicsStreamedUniformRawData::Raw(
                                GraphicsStreamUniformRawDataStatic::new(
                                    uni.mapped_memory.get_mem(
                                        GRAPHICS_MAX_UNIFORM_RENDER_COUNT
                                            * GRAPHICS_DEFAULT_UNIFORM_SIZE,
                                    ),
                                    Box::new(stream_data.cur_stream_uniform_buffers.clone()),
                                ),
                            ))
                        }),
                );

                (
                    GraphicsStreamedData::new(
                        GraphicsStreamVertices::Static(GraphicsStreamVerticesStatic::new(
                            mem,
                            Box::new(stream_data.cur_stream_vertex_buffer.clone()),
                        )),
                        graphics_uniform_data,
                    ),
                    InUseDataPerBackend::Vulkan(stream_data),
                    InUseDataPerBackend::Vulkan(next_stream_data),
                )
            }
            GraphicsBackendType::Null(_) => (
                GraphicsStreamedData::new(
                    GraphicsStreamVertices::Vec({
                        let mut res = Vec::with_capacity(4096);
                        res.resize_with(4096, Default::default);
                        res
                    }),
                    {
                        let mut res = PoolVec::new_without_pool();
                        res.resize_with(64, || {
                            GraphicsStreamedUniformData::new(
                                GraphicsStreamedUniformRawData::Vector(vec![0; 1024]),
                            )
                        });
                        res
                    },
                ),
                InUseDataPerBackend::Null,
                InUseDataPerBackend::Null,
            ),
        };

        sender.send(BackendThreadFrontendEvent::BuffersFromBackend {
            streamed_data: stream_data.try_into_sync_send_wrapper()?,
            cmds: Vec::new(),
        })?;

        'outer: while let Ok(event) = events.recv() {
            match event {
                BackendThreadBackendEvent::Init { .. } => {
                    panic!("backend is already initialized")
                }
                BackendThreadBackendEvent::FinishInit { .. } => {
                    panic!("backend is already initialized")
                }
                BackendThreadBackendEvent::RunCmds {
                    mut cmds,
                    stream_data: stream_data_cmd,
                } => {
                    let stream_data_cmd =
                        GraphicsStreamedData::from_sync_send_wrapper(stream_data_cmd);
                    match &in_use_data {
                        InUseDataPerBackend::Vulkan(data) => {
                            let GraphicsBackendType::Vulkan(backend) = &mut backend else {
                                return Err(anyhow!("not a vulkan backend"));
                            };
                            backend.set_stream_data_in_use(&stream_data_cmd, data)?;
                        }
                        InUseDataPerBackend::Null => {
                            // nothing to do
                        }
                    }
                    in_use_data = next_in_use_data;
                    let backend_ref = backend.as_mut();
                    backend_ref.start_commands(cmds.len());

                    for cmd in cmds.drain(..) {
                        backend_ref.run_command(cmd)?;
                    }
                    backend_ref.end_commands()?;

                    (stream_data, next_in_use_data) = match &mut backend {
                        GraphicsBackendType::Vulkan(backend) => {
                            let stream_data = backend.get_stream_data()?;

                            let mem = unsafe {
                                stream_data.cur_stream_vertex_buffer.memories[0]
                                    .mapped_memory
                                    .get_mem_typed::<GlVertex>(StreamDataMax::MaxVertices as usize)
                            };

                            let mut graphics_uniform_data =
                                backend.props.graphics_uniform_buffers.new();
                            graphics_uniform_data.extend(
                                stream_data.cur_stream_uniform_buffers.memories.iter().map(
                                    |uni| unsafe {
                                        GraphicsStreamedUniformData::new(
                                            GraphicsStreamedUniformRawData::Raw(
                                                GraphicsStreamUniformRawDataStatic::new(
                                                    uni.mapped_memory.get_mem(
                                                        GRAPHICS_MAX_UNIFORM_RENDER_COUNT
                                                            * GRAPHICS_DEFAULT_UNIFORM_SIZE,
                                                    ),
                                                    Box::new(
                                                        stream_data
                                                            .cur_stream_uniform_buffers
                                                            .clone(),
                                                    ),
                                                ),
                                            ),
                                        )
                                    },
                                ),
                            );

                            (
                                GraphicsStreamedData::new(
                                    GraphicsStreamVertices::Static(
                                        GraphicsStreamVerticesStatic::new(
                                            mem,
                                            Box::new(stream_data.cur_stream_vertex_buffer.clone()),
                                        ),
                                    ),
                                    graphics_uniform_data,
                                ),
                                InUseDataPerBackend::Vulkan(stream_data),
                            )
                        }
                        GraphicsBackendType::Null(_) => (
                            GraphicsStreamedData::new(
                                GraphicsStreamVertices::Vec({
                                    let mut res = Vec::with_capacity(4096);
                                    res.resize_with(4096, Default::default);
                                    res
                                }),
                                {
                                    let mut res = PoolVec::new_without_pool();
                                    res.resize_with(64, || {
                                        GraphicsStreamedUniformData::new(
                                            GraphicsStreamedUniformRawData::Vector(vec![0; 1024]),
                                        )
                                    });
                                    res
                                },
                            ),
                            InUseDataPerBackend::Null,
                        ),
                    };

                    sender.send(BackendThreadFrontendEvent::BuffersFromBackend {
                        streamed_data: stream_data.try_into_sync_send_wrapper()?,
                        cmds,
                    })?;
                }
                BackendThreadBackendEvent::Stop => break 'outer,
                BackendThreadBackendEvent::AttachFrameFetcher { name, fetcher } => {
                    backend.as_mut().attach_frame_fetcher(name, fetcher)
                }
                BackendThreadBackendEvent::DetachFrameFetcher { name } => {
                    backend.as_mut().detach_frame_fetcher(name)
                }
                BackendThreadBackendEvent::WindowCreateNtfy { oneshot: sender } => {
                    sender.send(match &backend {
                        GraphicsBackendType::Vulkan(backend) => {
                            BackendThreadInitFromMainThread::Vulkan(backend.get_main_thread_data())
                        }
                        GraphicsBackendType::Null(_) => BackendThreadInitFromMainThread::Null,
                    })?;
                }
                BackendThreadBackendEvent::WindowDestroyNtfy(sender) => {
                    match &mut backend {
                        GraphicsBackendType::Vulkan(backend) => {
                            backend.surface_lost()?;
                        }
                        GraphicsBackendType::Null(_) => {}
                    }
                    sender.send(())?;
                }
                BackendThreadBackendEvent::WindowCreated { main_thread_init } => match &mut backend
                {
                    GraphicsBackendType::Vulkan(backend) => {
                        let BackendThreadMainThreadInit::Vulkan(data) = main_thread_init else {
                            return Err(anyhow!("created window must be for vulkan type."));
                        };
                        backend.set_from_main_thread(data)?;
                    }
                    GraphicsBackendType::Null(_) => {}
                },
                BackendThreadBackendEvent::WaitForIdle(sender) => {
                    sender.send(())?;
                }
            }
        }

        Ok(())
    }
}
