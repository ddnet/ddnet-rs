use std::{rc::Rc, sync::Arc};

use hiarc::{Hiarc, hiarc_safer_rc_refcell};

use crate::{
    scene_object_shared::SceneObjectInner, stream::StreamDecoder, stream_object::StreamObject,
    types::StreamPlayProps,
};

/// handles streamed sound creation
#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct StreamObjectHandle {
    id_gen: u128,

    scene: Rc<SceneObjectInner>,
}

#[hiarc_safer_rc_refcell]
impl StreamObjectHandle {
    pub fn new(scene: Rc<SceneObjectInner>) -> Self {
        Self { id_gen: 0, scene }
    }

    pub fn create<F: StreamDecoder + hiarc::HiarcTrait>(
        &mut self,
        handle: Arc<F>,
        props: StreamPlayProps,
    ) -> StreamObject {
        let id = self.id_gen;
        self.id_gen += 1;

        StreamObject::new(id, handle, props, self.scene.clone())
    }
}
