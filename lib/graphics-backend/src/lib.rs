//#![deny(missing_docs)]
//#![deny(warnings)]
//#![deny(clippy::nursery)]
//#![deny(clippy::pedantic)]
//#![deny(clippy::all)]
// allowed
#![allow(clippy::module_inception)]
#![allow(clippy::eq_op)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::redundant_pattern_matching)]
// bcs vulkan backend uses too much unsafe
#![allow(clippy::missing_safety_doc)]
// temporary
#![allow(clippy::field_reassign_with_default)]

pub mod backend;
mod backend_mt;
pub mod backend_thread;
mod backends;
pub mod cache;
pub mod checker;
pub mod window;

#[cfg(test)]
mod tests {
    use std::{rc::Rc, sync::Arc};

    use base::benchmark::Benchmark;
    use base_fs::filesys::FileSystem;
    use base_http::http::HttpClient;
    use base_io::io::{Io, IoFileSys};
    use config::config::ConfigBackend;
    use graphics_backend_traits::{
        frame_fetcher_plugin::{
            BackendFrameFetcher, BackendPresentedImageDataRgba, FetchCanvasError, FetchCanvasIndex,
        },
        traits::GraphicsBackendInterface,
        types::BackendCommands,
    };
    use graphics_base_traits::traits::GraphicsStreamedData;
    use graphics_types::{
        commands::{
            AllCommands, CommandClear, CommandOffscreenCanvasCreate, CommandRender,
            CommandSwitchCanvasMode, CommandSwitchCanvasModeType, CommandsMisc, CommandsRender,
            CommandsRenderStream, PrimType,
        },
        rendering::{ColorRgba, StateTexture},
    };

    use crate::{
        backend::{
            GraphicsBackend, GraphicsBackendBase, GraphicsBackendIoLoading, GraphicsBackendLoading,
        },
        backends::vulkan::compiler::compiler::ShaderCompiler,
    };

    fn prepare_backend(
        thread_count: usize,
        config_gl: ConfigBackend,
    ) -> (Rc<GraphicsBackend>, GraphicsStreamedData) {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
        std::env::set_current_dir(workspace_root).unwrap();
        let io = IoFileSys::new(|rt| {
            Arc::new(
                FileSystem::new(rt, "ddnet-test", "ddnet-test", "ddnet-test", "ddnet-test")
                    .unwrap(),
            )
        });
        let tp = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(thread_count)
                .build()
                .unwrap(),
        );

        let config_gfx = config::config::ConfigGfx::default();
        let config_wnd = config::config::ConfigWindow::default();
        let io_loading = GraphicsBackendIoLoading::new(&config_gfx, &io);
        let mut config_dbg = config::config::ConfigDebug::default();
        config_dbg.bench = true;
        config_dbg.gfx = config::config::GfxDebugModes::All;

        let bench = Benchmark::new(true);
        let backend_loading = GraphicsBackendLoading::new(
            &config_gfx,
            &config_dbg,
            &config_gl,
            crate::window::BackendRawDisplayHandle::Headless,
            None,
            io.clone(),
        )
        .unwrap();
        bench.bench("backend loading");
        let (backend_base, stream_data) = GraphicsBackendBase::new(
            io_loading,
            backend_loading,
            &tp,
            crate::window::BackendWindow::Headless {
                width: config_wnd.window_width as u32,
                height: config_wnd.window_height as u32,
            },
        )
        .unwrap();
        bench.bench("backend base init");
        let backend = GraphicsBackend::new(backend_base);
        bench.bench("backend init");

        (backend, stream_data)
    }

    #[derive(Debug)]
    struct FrameFetcher {}

    impl BackendFrameFetcher for FrameFetcher {
        fn next_frame(&self, frame_data: BackendPresentedImageDataRgba) {
            assert_eq!(
                format!(
                    "{} - {:?}",
                    frame_data.dest_data_buffer.len(),
                    &frame_data.dest_data_buffer[0..4]
                ),
                // 20 (w) * 10 (h) * 4 (rgba), red pixel
                "800 - [255, 0, 0, 0]"
            );
        }

        fn current_fetch_index(&self) -> FetchCanvasIndex {
            FetchCanvasIndex::Offscreen(0)
        }

        fn fetch_err(&self, err: FetchCanvasError) {
            panic!("{:?}", err)
        }
    }

    #[test]
    fn vk_backend() {
        let (backend, stream_data) = prepare_backend(1, Default::default());

        backend
            .attach_frame_fetcher("noname".to_string(), Arc::new(FrameFetcher {}))
            .unwrap();

        let cmds = BackendCommands::default();

        cmds.add_cmd(AllCommands::Render(CommandsRender::Clear(CommandClear {
            color: ColorRgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            },
            force_clear: true,
        })));

        stream_data.add_vertices(&[Default::default(); 4]);
        cmds.add_cmd(AllCommands::Render(CommandsRender::Stream(
            CommandsRenderStream::Render(CommandRender {
                state: Default::default(),
                texture_index: StateTexture::None,
                prim_type: PrimType::Lines,
                prim_count: 4,
                vertices_offset: 0,
            }),
        )));

        cmds.add_cmd(AllCommands::Misc(CommandsMisc::OffscreenCanvasCreate(
            CommandOffscreenCanvasCreate {
                offscreen_index: 0,
                width: 20,
                height: 10,
                has_multi_sampling: None,
            },
        )));

        cmds.add_cmd(AllCommands::Misc(CommandsMisc::SwitchCanvas(
            CommandSwitchCanvasMode {
                mode: CommandSwitchCanvasModeType::Offscreen { id: 0 },
            },
        )));

        cmds.add_cmd(AllCommands::Render(CommandsRender::Clear(CommandClear {
            color: ColorRgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            },
            force_clear: true,
        })));

        stream_data.add_vertices(&[Default::default(); 4]);
        cmds.add_cmd(AllCommands::Render(CommandsRender::Stream(
            CommandsRenderStream::Render(CommandRender {
                state: Default::default(),
                texture_index: StateTexture::None,
                prim_type: PrimType::Lines,
                prim_count: 4,
                vertices_offset: 0,
            }),
        )));

        cmds.add_cmd(AllCommands::Misc(CommandsMisc::Swap));

        backend.run_cmds(&cmds, &stream_data);
    }

    #[test]
    fn vk_multi_sampling() {
        let mut config_gl: ConfigBackend = Default::default();
        config_gl.msaa_samples = 8;
        let (backend, stream_data) = prepare_backend(1, config_gl);

        backend
            .attach_frame_fetcher("noname".to_string(), Arc::new(FrameFetcher {}))
            .unwrap();

        let cmds = BackendCommands::default();

        cmds.add_cmd(AllCommands::Render(CommandsRender::Clear(CommandClear {
            color: ColorRgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            },
            force_clear: true,
        })));

        cmds.add_cmd(AllCommands::Misc(CommandsMisc::OffscreenCanvasCreate(
            CommandOffscreenCanvasCreate {
                offscreen_index: 0,
                width: 20,
                height: 10,
                has_multi_sampling: None,
            },
        )));

        cmds.add_cmd(AllCommands::Misc(CommandsMisc::SwitchCanvas(
            CommandSwitchCanvasMode {
                mode: CommandSwitchCanvasModeType::Offscreen { id: 0 },
            },
        )));

        cmds.add_cmd(AllCommands::Render(CommandsRender::Clear(CommandClear {
            color: ColorRgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            },
            force_clear: true,
        })));

        cmds.add_cmd(AllCommands::Misc(CommandsMisc::Swap));

        cmds.add_cmd(AllCommands::Misc(CommandsMisc::ConsumeMultiSamplingTargets));
        cmds.add_cmd(AllCommands::Misc(CommandsMisc::Swap));

        backend.run_cmds(&cmds, &stream_data);
    }

    #[test]
    fn vk_frames() {
        let (backend, stream_data) = prepare_backend(1, Default::default());

        let cmds = BackendCommands::default();

        let add = || {
            stream_data.add_vertices(&[Default::default(); 4]);
            cmds.add_cmd(AllCommands::Render(CommandsRender::Stream(
                CommandsRenderStream::Render(CommandRender {
                    state: Default::default(),
                    texture_index: StateTexture::None,
                    prim_type: PrimType::Lines,
                    prim_count: 4,
                    vertices_offset: 0,
                }),
            )));

            cmds.add_cmd(AllCommands::Misc(CommandsMisc::SwitchCanvas(
                CommandSwitchCanvasMode {
                    mode: CommandSwitchCanvasModeType::Onscreen,
                },
            )));

            stream_data.add_vertices(&[Default::default(); 4]);
            cmds.add_cmd(AllCommands::Render(CommandsRender::Stream(
                CommandsRenderStream::Render(CommandRender {
                    state: Default::default(),
                    texture_index: StateTexture::None,
                    prim_type: PrimType::Lines,
                    prim_count: 4,
                    vertices_offset: 0,
                }),
            )));

            cmds.add_cmd(AllCommands::Misc(CommandsMisc::SwitchCanvas(
                CommandSwitchCanvasMode {
                    mode: CommandSwitchCanvasModeType::Onscreen,
                },
            )));

            stream_data.add_vertices(&[Default::default(); 4]);
            cmds.add_cmd(AllCommands::Render(CommandsRender::Stream(
                CommandsRenderStream::Render(CommandRender {
                    state: Default::default(),
                    texture_index: StateTexture::None,
                    prim_type: PrimType::Lines,
                    prim_count: 4,
                    vertices_offset: 0,
                }),
            )));

            cmds.add_cmd(AllCommands::Misc(CommandsMisc::SwitchCanvas(
                CommandSwitchCanvasMode {
                    mode: CommandSwitchCanvasModeType::Onscreen,
                },
            )));

            stream_data.add_vertices(&[Default::default(); 4]);
            cmds.add_cmd(AllCommands::Render(CommandsRender::Stream(
                CommandsRenderStream::Render(CommandRender {
                    state: Default::default(),
                    texture_index: StateTexture::None,
                    prim_type: PrimType::Lines,
                    prim_count: 4,
                    vertices_offset: 0,
                }),
            )));

            cmds.add_cmd(AllCommands::Misc(CommandsMisc::Swap));
        };
        add();
        add();
        add();
        add();

        backend.run_cmds(&cmds, &stream_data);
    }

    #[test]
    fn shader_compile() {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
        std::env::set_current_dir(workspace_root).unwrap();
        let io = Io::new(
            |rt| {
                Arc::new(
                    FileSystem::new(rt, "ddnet-test", "ddnet-test", "ddnet-test", "ddnet-test")
                        .unwrap(),
                )
            },
            Arc::new(HttpClient::new()),
        );

        let fs = io.fs.clone();
        let backend_files = io.rt.spawn(async move {
            let mut compiled_files: Vec<(String, Vec<u32>)> = Default::default();
            let mut compiler = ShaderCompiler::new(
                crate::backends::vulkan::compiler::compiler::ShaderCompilerType::WgslInSpvOut,
                fs,
            )
            .await;
            compiler
                .compile("shader/wgsl".as_ref(), "compile.json".as_ref())
                .await?;
            for (name, file) in compiler.shader_files.iter() {
                println!("compiling: {}", name);
                compiled_files.push((name.clone(), file.clone()));
            }

            Ok(compiled_files)
        });
        let files = backend_files.get_storage().unwrap();
        assert!(!files.is_empty());
        /*for (name, file) in files {
            let mut f = std::fs::File::create(name).unwrap();

            // Convert each u32 in the vector to bytes and write to the file
            for val in file.iter() {
                std::io::Write::write_all(&mut f, &val.to_le_bytes()).unwrap();
            }
        }*/
    }
}
