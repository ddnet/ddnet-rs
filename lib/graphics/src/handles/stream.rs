pub mod stream {
    use std::marker::PhantomData;

    use graphics_base_traits::traits::GraphicsStreamedData;
    use graphics_types::{
        commands::{
            AllCommands, CommandRender, CommandsRender, CommandsRenderStream, PrimType,
            RenderCommand, RenderSpriteInfo,
        },
        rendering::{GlVertex, RenderMode, State},
        types::DrawModes,
    };
    use hiarc::{HiFnMut, HiFnMutBase, HiFnOnce};
    use hiarc::{Hiarc, hi_closure};

    use crate::handles::{
        backend::backend::GraphicsBackendHandle,
        canvas::canvas::OffscreenCanvas,
        stream_types::{StreamedLine, StreamedQuad, StreamedTriangle},
        texture::texture::{TextureContainer, TextureType},
    };

    fn flush_vertices_impl<T>(
        stream_data: &GraphicsStreamedData,
        state: &State,
        draw_mode: DrawModes,
        vertices_offset: usize,
        cmd: &mut T,
    ) -> bool
    where
        T: RenderCommand,
    {
        let prim_type: PrimType;
        let prim_count: usize;

        let num_verts: usize = stream_data.vertices_count() - vertices_offset;

        if num_verts == 0 {
            return false;
        }

        match draw_mode {
            DrawModes::Quads => {
                prim_type = PrimType::Quads;
                prim_count = num_verts / 4;
            }
            DrawModes::Lines => {
                prim_type = PrimType::Lines;
                prim_count = num_verts / 2;
            }
            DrawModes::Triangles => {
                prim_type = PrimType::Triangles;
                prim_count = num_verts / 3;
            }
        }

        cmd.set_state(*state);

        cmd.set_prim_type(prim_type);
        cmd.set_prim_count(prim_count);

        true
    }

    pub struct StreamedUniforms<'a, T: hiarc::HiarcTrait> {
        stream_handle: &'a GraphicsStreamHandle,
        handle: &'a GraphicsStreamedData,
        instance: usize,

        flusher: &'a mut dyn HiFnMutBase<(usize, usize), ()>,

        phantom_data: PhantomData<T>,
    }

    impl<'a, T: hiarc::HiarcTrait> StreamedUniforms<'a, T> {
        fn alloc_instance(
            stream_handle: &'a GraphicsStreamHandle,
            handle: &'a GraphicsStreamedData,
        ) -> usize {
            let mut instance = handle.allocate_uniform_instance();
            if instance.is_err() {
                stream_handle
                    .backend_handle
                    .run_backend_buffer(&stream_handle.stream_data);
                instance = handle.allocate_uniform_instance();
            }

            instance.unwrap()
        }

        pub fn new(
            stream_handle: &'a GraphicsStreamHandle,
            handle: &'a GraphicsStreamedData,
            flusher: &'a mut dyn HiFnMutBase<(usize, usize), ()>,
        ) -> Self {
            let instance = Self::alloc_instance(stream_handle, handle);
            Self {
                instance,
                handle,
                stream_handle,
                flusher,
                phantom_data: Default::default(),
            }
        }

        pub fn add(&mut self, info: T) {
            let (used_count, should_flush) = self.handle.add_uniform::<T>(self.instance, info);
            if should_flush {
                // flush
                self.flusher.call_mut((self.instance, used_count));
                self.instance = Self::alloc_instance(self.stream_handle, self.handle);
            }
        }

        fn flush_impl(&mut self, new_instance: bool) {
            let (count, _) = self.handle.uniform_info::<T>(self.instance);
            if count > 0 {
                self.flusher.call_mut((self.instance, count));
                if new_instance {
                    self.instance = Self::alloc_instance(self.stream_handle, self.handle);
                }
            }
        }

        pub fn flush(&mut self) {
            self.flush_impl(true)
        }
    }

    impl<T: hiarc::HiarcTrait> Drop for StreamedUniforms<'_, T> {
        fn drop(&mut self) {
            self.flush_impl(false);
        }
    }

    pub type StreamedSprites<'a> = StreamedUniforms<'a, RenderSpriteInfo>;

    pub struct StreamHandle<'a, const N: usize> {
        handle: &'a GraphicsStreamHandle,
        state: State,
        render_mode: RenderMode,
        vertices_offset: usize,
        draw_mode: DrawModes,
        texture: TextureType,
    }

    impl<const N: usize> StreamHandle<'_, N> {
        pub fn add_vertices(&mut self, add_vertices: [GlVertex; N]) {
            let needs_flush = self.handle.stream_data.is_full(N);
            if needs_flush {
                self.handle.flush_vertices(
                    &self.state,
                    self.texture.clone(),
                    &self.render_mode,
                    self.vertices_offset,
                    self.draw_mode,
                );
                self.vertices_offset = 0;

                self.handle
                    .backend_handle
                    .run_backend_buffer(&self.handle.stream_data);
            }

            let stream_data = &self.handle.stream_data;
            stream_data.add_vertices(&add_vertices);
        }

        pub fn set_render_mode(&mut self, render_mode: RenderMode) {
            self.render_mode = render_mode;
        }

        pub fn set_texture_ty(&mut self, texture: TextureType) {
            self.texture = texture;
        }

        pub fn set_texture(&mut self, tex_index: &TextureContainer) {
            self.texture = tex_index.into();
        }

        pub fn set_color_attachment_texture(&mut self) {
            self.texture = TextureType::ColorAttachmentOfPreviousPass;
        }

        pub fn set_offscreen_attachment_texture(&mut self, offscreen_canvas: &OffscreenCanvas) {
            self.texture = TextureType::ColorAttachmentOfOffscreen(offscreen_canvas.clone());
        }
    }

    impl<const N: usize> Drop for StreamHandle<'_, N> {
        fn drop(&mut self) {
            self.handle.flush_vertices(
                &self.state,
                self.texture.clone(),
                &self.render_mode,
                self.vertices_offset,
                self.draw_mode,
            );
        }
    }

    pub type LinesStreamHandle<'a> = StreamHandle<'a, 2>;
    pub type TriangleStreamHandle<'a> = StreamHandle<'a, 3>;
    pub type QuadStreamHandle<'a> = StreamHandle<'a, 4>;

    #[derive(Debug, Hiarc)]
    pub struct GraphicsStreamHandle {
        stream_data: GraphicsStreamedData,

        backend_handle: GraphicsBackendHandle,
    }

    impl Clone for GraphicsStreamHandle {
        fn clone(&self) -> Self {
            Self {
                stream_data: self.stream_data.clone(),
                backend_handle: self.backend_handle.clone(),
            }
        }
    }

    impl GraphicsStreamHandle {
        pub fn new(
            stream_data: GraphicsStreamedData,
            backend_handle: GraphicsBackendHandle,
        ) -> Self {
            Self {
                stream_data,
                backend_handle,
            }
        }

        /// use [`graphics::handles::stream_types::StreamedLine`] to build a line
        pub fn stream_lines<'a, F>(&'a self, draw: F, state: State)
        where
            F: HiFnOnce<LinesStreamHandle<'a>, ()>,
        {
            let vertices_offset = self.stream_data.vertices_count();
            let stream_handle = LinesStreamHandle {
                draw_mode: DrawModes::Lines,
                handle: self,
                render_mode: RenderMode::default(),
                state,
                vertices_offset,
                texture: Default::default(),
            };
            draw.call_once(stream_handle);
        }

        /// use [`graphics::handles::stream_types::StreamedQuad`] to build a quad
        pub fn stream_quads<'a, F>(&'a self, draw: F, state: State)
        where
            F: HiFnOnce<QuadStreamHandle<'a>, ()>,
        {
            let vertices_offset = self.stream_data.vertices_count();
            let stream_handle = QuadStreamHandle {
                draw_mode: DrawModes::Quads,
                handle: self,
                render_mode: RenderMode::default(),
                state,
                vertices_offset,
                texture: Default::default(),
            };
            draw.call_once(stream_handle);
        }

        /// use [`graphics::handles::stream_types::StreamedTriangle`] to build a triangle
        pub fn stream_triangles<'a, F>(&'a self, draw: F, state: State)
        where
            F: HiFnOnce<TriangleStreamHandle<'a>, ()>,
        {
            let vertices_offset = self.stream_data.vertices_count();
            let stream_handle = TriangleStreamHandle {
                draw_mode: DrawModes::Triangles,
                handle: self,
                render_mode: RenderMode::default(),
                state,
                vertices_offset,
                texture: Default::default(),
            };
            draw.call_once(stream_handle);
        }

        /// Render [`graphics::handles::stream_types::StreamedLine`]s.
        pub fn render_lines(&self, lines: &[StreamedLine], state: State) {
            self.stream_lines(
                hi_closure!([
                        lines: &[StreamedLine],
                    ],
                    |mut stream_handle: LinesStreamHandle<'_>| -> () {
                        for line in lines.iter().copied() {
                            stream_handle.add_vertices(line.into());
                        }
                    }
                ),
                state,
            );
        }

        /// Render [`graphics::handles::stream_types::StreamedQuad`]s.
        pub fn render_quads(&self, quads: &[StreamedQuad], state: State, texture: TextureType) {
            let texture = &texture;
            self.stream_quads(
                hi_closure!([
                        quads: &[StreamedQuad],
                        texture: &TextureType,
                    ],
                    |mut stream_handle: QuadStreamHandle<'_>| -> () {
                        stream_handle.set_texture_ty(texture.clone());
                        for quad in quads.iter().copied() {
                            stream_handle.add_vertices(quad.into());
                        }
                    }
                ),
                state,
            );
        }

        /// Render [`graphics::handles::stream_types::StreamedTriangle`]s.
        pub fn render_triangles(
            &self,
            triangles: &[StreamedTriangle],
            state: State,
            texture: TextureType,
        ) {
            let texture = &texture;
            self.stream_triangles(
                hi_closure!([
                        triangles: &[StreamedTriangle],
                        texture: &TextureType,
                    ],
                    |mut stream_handle: TriangleStreamHandle<'_>| -> () {
                        stream_handle.set_texture_ty(texture.clone());
                        for triangle in triangles.iter().copied() {
                            stream_handle.add_vertices(triangle.into());
                        }
                    }
                ),
                state,
            );
        }

        fn flush_vertices(
            &self,
            state: &State,
            texture_index: TextureType,
            render_mode: &RenderMode,
            vertices_offset: usize,
            draw_mode: DrawModes,
        ) {
            let mut cmd = CommandRender::new(PrimType::Lines, texture_index.into());
            if flush_vertices_impl(
                &self.stream_data,
                state,
                draw_mode,
                vertices_offset,
                &mut cmd,
            ) {
                cmd.vertices_offset = vertices_offset;

                let render_cmd = match render_mode {
                    RenderMode::Standard => CommandsRenderStream::Render(cmd),
                    RenderMode::Blur {
                        blur_radius,
                        scale,
                        blur_color,
                    } => CommandsRenderStream::RenderBlurred {
                        cmd,
                        blur_radius: *blur_radius,
                        scale: *scale,
                        blur_color: *blur_color,
                    },
                };

                self.backend_handle
                    .add_cmd(AllCommands::Render(CommandsRender::Stream(render_cmd)));
            }
        }

        pub fn stream_data(&self) -> &GraphicsStreamedData {
            &self.stream_data
        }

        pub fn fill_sprites_uniform_instance<C, F>(&self, construct: C, mut flusher: F)
        where
            for<'a> C: HiFnOnce<StreamedSprites<'a>, ()>,
            F: HiFnMut<(usize, usize), ()>,
        {
            let handle = &self.stream_data;
            let stream_handle = StreamedSprites::new(self, handle, &mut flusher);
            construct.call_once(stream_handle);
        }

        pub fn fill_uniform_instance<C, F, T: hiarc::HiarcTrait>(
            &self,
            construct: C,
            mut flusher: F,
        ) where
            for<'a> C: HiFnOnce<StreamedUniforms<'a, T>, ()>,
            F: HiFnMut<(usize, usize), ()>,
        {
            let handle = &self.stream_data;
            let stream_handle = StreamedUniforms::<'_, T>::new(self, handle, &mut flusher);
            construct.call_once(stream_handle);
        }
    }
}
