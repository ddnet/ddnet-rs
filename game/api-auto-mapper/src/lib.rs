use std::cell::RefCell;
use std::rc::Rc;

use api_wasm_macros::{guest_func_call_from_host_auto, impl_guest_functions_auto_mapper};
use editor_interface::auto_mapper::{
    AutoMapperInputModes, AutoMapperInterface, AutoMapperModes, AutoMapperOutputModes,
};

use api::read_param_from_host_ex;
use api::upload_return_val;

unsafe extern "Rust" {
    /// Returns an instance of the auto mapper
    fn mod_auto_mapper_new() -> Result<Box<dyn AutoMapperInterface>, String>;
}

pub struct ApiAutoMapper {
    /// The wasm state object
    state: Rc<RefCell<Option<Box<dyn AutoMapperInterface>>>>,
}

thread_local! {
static API_AUTO_MAPPER: once_cell::unsync::Lazy<ApiAutoMapper> = once_cell::unsync::Lazy::new(|| ApiAutoMapper { state: Default::default(), });
}

impl ApiAutoMapper {
    fn create(&self) -> Result<(), String> {
        let state = unsafe { mod_auto_mapper_new()? };
        *self.state.borrow_mut() = Some(state);
        Ok(())
    }
}

#[unsafe(no_mangle)]
pub fn auto_mapper_new() {
    let res = API_AUTO_MAPPER.with(|g| g.create());
    upload_return_val(res);
}

#[unsafe(no_mangle)]
pub fn auto_mapper_drop() {
    API_AUTO_MAPPER.with(|g| *g.state.borrow_mut() = None);
}

#[impl_guest_functions_auto_mapper]
impl AutoMapperInterface for ApiAutoMapper {
    #[guest_func_call_from_host_auto(option)]
    fn supported_modes(&self) -> Vec<AutoMapperModes> {}

    #[guest_func_call_from_host_auto(option)]
    fn run(
        &mut self,
        seed: u64,
        input: AutoMapperInputModes,
    ) -> Result<AutoMapperOutputModes, String> {
    }
}
