use hiarc::{Hiarc, hiarc_safer_rc_refcell};

use crate::{
    backend_handle::SoundBackendHandle, commands::SoundSceneCreateProps, scene_object::SceneObject,
};

/// allocates new scene objects
#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct SoundSceneHandle {
    backend_handle: SoundBackendHandle,
    id_gen: u128,

    off_air_id_gen: u128,
}

#[hiarc_safer_rc_refcell]
impl SoundSceneHandle {
    pub fn new(backend_handle: SoundBackendHandle) -> Self {
        Self {
            backend_handle,
            id_gen: 0,
            off_air_id_gen: 0,
        }
    }

    pub fn next_offair_id(&mut self) -> u128 {
        let id = self.off_air_id_gen;
        self.off_air_id_gen += 1;
        id
    }

    pub fn create(&mut self, props: SoundSceneCreateProps) -> SceneObject {
        let id = self.id_gen;
        self.id_gen += 1;

        SceneObject::new(id, props, self.backend_handle.clone())
    }
}
