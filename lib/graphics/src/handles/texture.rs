pub mod texture {
    use anyhow::anyhow;
    use graphics_types::{
        commands::{
            AllCommands, CommandTextureCreate, CommandTextureDestroy, CommandTextureUpdate,
            CommandsMisc,
        },
        rendering::{StateTexture, StateTexture2dArray},
        types::{GraphicsBackendMemory, GraphicsMemoryAllocationType},
    };
    use hiarc::{Hiarc, hiarc_safer_rc_refcell};

    use crate::handles::{
        backend::backend::GraphicsBackendHandle, canvas::canvas::OffscreenCanvas,
    };

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct GraphicsTextureHandle {
        id_gen: u128,

        backend_handle: GraphicsBackendHandle,
    }

    #[hiarc_safer_rc_refcell]
    impl GraphicsTextureHandle {
        pub(crate) fn new(backend_handle: GraphicsBackendHandle) -> Self {
            Self {
                id_gen: Default::default(),

                backend_handle,
            }
        }

        fn load_texture_impl(
            &mut self,
            data: GraphicsBackendMemory,
            _tex_name: &str,
        ) -> anyhow::Result<u128> {
            // grab texture
            self.id_gen += 1;
            let tex_index = self.id_gen;

            let cmd = CommandTextureCreate {
                texture_index: tex_index,
                data,
            };

            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::TextureCreate(cmd)));

            Ok(tex_index)
        }

        pub fn load_texture_rgba_u8(
            &mut self,
            data: GraphicsBackendMemory,
            tex_name: &str,
        ) -> anyhow::Result<TextureContainer> {
            anyhow::ensure!(
                matches!(
                    data.usage(),
                    GraphicsMemoryAllocationType::TextureRgbaU8 { .. }
                ),
                "{tex_name} was not a rgba u8 image"
            );
            let tex_index = self.load_texture_impl(data, tex_name)?;
            Ok(TextureContainer::new(
                tex_index,
                self.backend_handle.clone(),
            ))
        }

        pub fn load_texture_2d_array_rgba_u8(
            &mut self,
            data: GraphicsBackendMemory,
            tex_name: &str,
        ) -> anyhow::Result<TextureContainer2dArray> {
            anyhow::ensure!(
                matches!(
                    data.usage(),
                    GraphicsMemoryAllocationType::TextureRgbaU82dArray { .. }
                ),
                "{tex_name} was not a 2d array rgba u8 texture"
            );
            let tex_index = self.load_texture_impl(data, tex_name)?;
            Ok(TextureContainer2dArray::new(
                tex_index,
                self.backend_handle.clone(),
            ))
        }
    }

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct TextureContainer {
        index: u128,
        backend_handle: GraphicsBackendHandle,
    }

    #[hiarc_safer_rc_refcell]
    impl Drop for TextureContainer {
        fn drop(&mut self) {
            let cmd = CommandTextureDestroy {
                texture_index: self.index,
            };
            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::TextureDestroy(cmd)));
        }
    }

    #[hiarc_safer_rc_refcell]
    impl TextureContainer {
        pub fn new(index: u128, backend_handle: GraphicsBackendHandle) -> Self {
            Self {
                index,
                backend_handle,
            }
        }

        /// updates the texture with specific limitations:
        /// - all commands that use this texture before this command was issued __might__ see the texture update too
        /// - all commands that are issued after this update are guaranteed to see the texture update
        pub fn update_texture(
            &self,
            x: isize,
            y: isize,
            width: usize,
            height: usize,
            data: Vec<u8>,
        ) -> anyhow::Result<()> {
            if width == 0 || height == 0 {
                return Err(anyhow!("width and/or height was 0"));
            }

            let cmd = CommandTextureUpdate {
                texture_index: self.index,
                x: x as i32,
                y: y as i32,
                width: width as u32,
                height: height as u32,
                data,
            };

            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::TextureUpdate(cmd)));

            Ok(())
        }

        pub fn get_index_unsafe(&self) -> u128 {
            self.index
        }
    }

    impl TextureContainer {
        pub fn tex_eq(&self, other: &Self) -> bool {
            self.get_index_unsafe() == other.get_index_unsafe()
        }
    }

    #[derive(Debug, Hiarc, Default, Clone)]
    pub enum TextureType {
        #[default]
        None,
        Texture(TextureContainer),
        ColorAttachmentOfPreviousPass,
        ColorAttachmentOfOffscreen(OffscreenCanvas),
    }

    impl From<TextureType> for StateTexture {
        fn from(val: TextureType) -> Self {
            match val {
                TextureType::None => StateTexture::None,
                TextureType::Texture(tex) => StateTexture::Texture(tex.get_index_unsafe()),
                TextureType::ColorAttachmentOfPreviousPass => {
                    StateTexture::ColorAttachmentOfPreviousPass
                }
                TextureType::ColorAttachmentOfOffscreen(offscreen_id) => {
                    StateTexture::ColorAttachmentOfOffscreen(offscreen_id.get_index_unsafe())
                }
            }
        }
    }

    impl<'a> From<&'a TextureContainer> for TextureType {
        fn from(value: &'a TextureContainer) -> Self {
            Self::Texture(value.clone())
        }
    }

    impl From<TextureContainer> for TextureType {
        fn from(value: TextureContainer) -> Self {
            Self::Texture(value)
        }
    }

    impl<'a> From<Option<&'a TextureContainer>> for TextureType {
        fn from(value: Option<&'a TextureContainer>) -> Self {
            match value {
                Some(value) => Self::Texture(value.clone()),
                None => Self::None,
            }
        }
    }

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc)]
    pub struct TextureContainer2dArray {
        index: u128,
        backend_handle: GraphicsBackendHandle,
    }

    #[hiarc_safer_rc_refcell]
    impl Drop for TextureContainer2dArray {
        fn drop(&mut self) {
            let cmd = CommandTextureDestroy {
                texture_index: self.index,
            };
            self.backend_handle
                .add_cmd(AllCommands::Misc(CommandsMisc::TextureDestroy(cmd)));
        }
    }

    #[hiarc_safer_rc_refcell]
    impl TextureContainer2dArray {
        pub fn new(index: u128, backend_handle: GraphicsBackendHandle) -> Self {
            Self {
                index,
                backend_handle,
            }
        }

        pub fn get_index_unsafe(&self) -> u128 {
            self.index
        }
    }

    impl TextureContainer2dArray {
        pub fn tex_eq(&self, other: &Self) -> bool {
            self.get_index_unsafe() == other.get_index_unsafe()
        }
    }

    #[derive(Debug, Hiarc, Default, Clone)]
    pub enum TextureType2dArray {
        #[default]
        None,
        Texture(TextureContainer2dArray),
    }

    impl From<TextureType2dArray> for StateTexture2dArray {
        fn from(val: TextureType2dArray) -> Self {
            match val {
                TextureType2dArray::None => StateTexture2dArray::None,
                TextureType2dArray::Texture(tex) => {
                    StateTexture2dArray::Texture(tex.get_index_unsafe())
                }
            }
        }
    }

    impl<'a> From<&'a TextureContainer2dArray> for TextureType2dArray {
        fn from(value: &'a TextureContainer2dArray) -> Self {
            Self::Texture(value.clone())
        }
    }

    impl From<TextureContainer2dArray> for TextureType2dArray {
        fn from(value: TextureContainer2dArray) -> Self {
            Self::Texture(value)
        }
    }

    impl<'a> From<Option<&'a TextureContainer2dArray>> for TextureType2dArray {
        fn from(value: Option<&'a TextureContainer2dArray>) -> Self {
            match value {
                Some(value) => Self::Texture(value.clone()),
                None => Self::None,
            }
        }
    }
}
