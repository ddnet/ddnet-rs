use std::{
    ffi::{CStr, CString},
    sync::Arc,
};

use anyhow::anyhow;
use ash::vk;
use config::config::GfxDebugModes;
use hiarc::Hiarc;

use crate::window::BackendDisplayRequirements;

#[derive(Clone, Hiarc)]
pub struct Instance {
    #[hiarc_skip_unsafe]
    pub vk_instance: ash::Instance,
    #[hiarc_skip_unsafe]
    pub vk_entry: ash::Entry,
}

impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance").finish()
    }
}

impl Instance {
    pub fn new(
        display_requirements: &BackendDisplayRequirements,
        dbg_mode: GfxDebugModes,
    ) -> anyhow::Result<Arc<Self>> {
        #[cfg(not(target_os = "macos"))]
        let entry = unsafe { ash::Entry::load() }?;
        #[cfg(target_os = "macos")]
        let entry = ash_molten::load();

        let extensions = &display_requirements.extensions;

        let instance = Self::create_vulkan_instance(
            dbg_mode,
            &entry,
            extensions,
            true,
            display_requirements.is_headless,
        )?;
        Ok(Arc::new(Self {
            vk_instance: instance,
            vk_entry: entry,
        }))
    }

    fn our_vklayers(
        dbg: GfxDebugModes,
        failed_previously: bool,
    ) -> std::collections::BTreeSet<String> {
        let mut our_layers: std::collections::BTreeSet<String> = Default::default();

        if (dbg == GfxDebugModes::Minimum || dbg == GfxDebugModes::All) && !failed_previously {
            our_layers.insert("VK_LAYER_KHRONOS_validation".to_string());
        }

        our_layers
    }

    fn get_vulkan_layers(
        dbg: GfxDebugModes,
        entry: &ash::Entry,
        failed_previously: bool,
    ) -> anyhow::Result<Vec<String>> {
        let vk_instance_layers = unsafe { entry.enumerate_instance_layer_properties()? };

        let req_layer_names = Self::our_vklayers(dbg, failed_previously);
        let mut vk_layers = Vec::<String>::new();
        for layer_name in &vk_instance_layers {
            let layer_name = unsafe {
                CStr::from_ptr(layer_name.layer_name.as_ptr())
                    .to_str()
                    .unwrap()
                    .to_string()
            };
            let it = req_layer_names.get(&layer_name);
            if let Some(_layer) = it {
                vk_layers.push(layer_name);
            }
        }

        Ok(vk_layers)
    }

    fn create_vulkan_instance(
        dbg: GfxDebugModes,
        entry: &ash::Entry,
        vk_extensions: &Vec<String>,
        try_debug_extensions: bool,
        is_headless: bool,
    ) -> anyhow::Result<ash::Instance> {
        let vk_layers = Self::get_vulkan_layers(dbg, entry, !try_debug_extensions)?;

        let mut layers_cstr_helper: Vec<CString> = Default::default();
        for layer in vk_layers {
            layers_cstr_helper
                .push(unsafe { CString::from_vec_unchecked(layer.as_bytes().to_vec()) });
        }

        let mut ext_cstr_helper: Vec<CString> = Default::default();
        for ext in vk_extensions {
            ext_cstr_helper.push(unsafe { CString::from_vec_unchecked(ext.as_bytes().to_vec()) });
        }

        if try_debug_extensions && (dbg == GfxDebugModes::Minimum || dbg == GfxDebugModes::All) {
            // debug message support
            ext_cstr_helper.push(CString::new(vk::EXT_DEBUG_UTILS_NAME.to_str().unwrap()).unwrap());
        }

        let app_name = CString::new("DDNet").unwrap();
        let app_vk_name = CString::new("DDNet-Vulkan").unwrap();

        let vk_app_info = vk::ApplicationInfo::default()
            .application_name(app_name.as_c_str())
            .application_version(1)
            .engine_name(app_vk_name.as_c_str())
            .engine_version(1)
            .api_version(if is_headless {
                vk::API_VERSION_1_2
            } else {
                vk::API_VERSION_1_1
            });

        let mut vk_instance_info = vk::InstanceCreateInfo::default();

        let mut features = vk::ValidationFeaturesEXT::default();
        let enabled_exts = [
            vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION,
            vk::ValidationFeatureEnableEXT::BEST_PRACTICES,
        ];
        if try_debug_extensions
            && (dbg == GfxDebugModes::AffectsPerformance || dbg == GfxDebugModes::All)
        {
            features = features.enabled_validation_features(&enabled_exts);

            vk_instance_info = vk_instance_info.push_next(&mut features);
        }

        let mut try_again: bool = false;

        let ext: Vec<_>;
        let layer: Vec<_>;

        let res = unsafe {
            entry.create_instance(
                {
                    vk_instance_info = vk_instance_info
                        .flags(vk::InstanceCreateFlags::empty())
                        .application_info(&vk_app_info);
                    ext = ext_cstr_helper
                        .iter()
                        .map(|s| s.as_ptr() as _)
                        .collect::<Vec<_>>();
                    layer = layers_cstr_helper
                        .iter()
                        .map(|s| s.as_ptr() as _)
                        .collect::<Vec<_>>();
                    if !ext.is_empty() {
                        vk_instance_info = vk_instance_info.enabled_extension_names(&ext);
                    }
                    if !layer.is_empty() {
                        vk_instance_info = vk_instance_info.enabled_layer_names(&layer);
                    }
                    &vk_instance_info
                },
                None,
            )
        };
        if let Err(res_err) = res {
            if res_err == vk::Result::ERROR_LAYER_NOT_PRESENT
                || res_err == vk::Result::ERROR_EXTENSION_NOT_PRESENT
            {
                try_again = true;
            } else {
                return Err(anyhow!("Creating instance failed: {res_err}"));
            }
        }

        if try_again && try_debug_extensions {
            return Self::create_vulkan_instance(dbg, entry, vk_extensions, false, is_headless);
        }

        drop(layers_cstr_helper);

        res.map_err(|err| {
            anyhow!("creating vk instance failed: {err}, extensions: {ext_cstr_helper:?}")
        })
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { self.vk_instance.destroy_instance(None) };
    }
}
