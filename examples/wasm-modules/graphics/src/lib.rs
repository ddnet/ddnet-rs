#![allow(clippy::too_many_arguments)]

pub mod graphics;
pub mod handles;

pub mod graphics_mt {
    pub use ::graphics::graphics_mt::*;
}

pub mod quad_container {
    pub use ::graphics::quad_container::*;
}
pub mod streaming {
    pub use ::graphics::streaming::*;
}
pub mod utils {
    pub use ::graphics::utils::*;
}
pub mod window_handling {
    pub use ::graphics::window_handling::*;
}
