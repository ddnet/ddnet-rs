use std::rc::Rc;

use hiarc::{Hiarc, hiarc_safer_rc_refcell};

use crate::{
    scene_object_shared::SceneObjectInner, sound_mt_types::SoundBackendMemory,
    sound_object::SoundObject,
};

/// handles sound creation
#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct SoundObjectHandle {
    id_gen: u128,

    scene: Rc<SceneObjectInner>,
}

#[hiarc_safer_rc_refcell]
impl SoundObjectHandle {
    pub fn new(scene: Rc<SceneObjectInner>) -> Self {
        Self { id_gen: 0, scene }
    }

    pub fn create(&mut self, mem: SoundBackendMemory) -> SoundObject {
        let id = self.id_gen;
        self.id_gen += 1;

        SoundObject::new(id, mem, self.scene.clone())
    }
}
