use std::rc::Rc;

use hiarc::{Hiarc, hiarc_safer_rc_refcell};
use math::math::vector::vec2;

use crate::{scene_object_shared::SceneObjectInner, sound_listener::SoundListener};

/// Allocates new sound listeners
#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct SoundListenerHandle {
    id_gen: u128,
    scene: Rc<SceneObjectInner>,
}

#[hiarc_safer_rc_refcell]
impl SoundListenerHandle {
    pub fn new(scene: Rc<SceneObjectInner>) -> Self {
        Self { id_gen: 0, scene }
    }

    pub fn create(&mut self, pos: vec2) -> SoundListener {
        let id = self.id_gen;
        self.id_gen += 1;

        SoundListener::new(id, pos, self.scene.clone())
    }
}
