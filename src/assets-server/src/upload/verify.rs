use std::{collections::HashMap, fmt::Debug};

use assets_base::{
    tar::read_tar_files,
    verify::{ogg_vorbis::verify_ogg_vorbis, txt::verify_txt},
    AssetUploadResponse,
};
use image_utils::png::{is_png_image_valid, PngValidatorOptions};

#[derive(Debug, Clone)]
pub enum AllowedResource {
    Png(PngValidatorOptions),
    PngCategory {
        per_category: HashMap<String, PngValidatorOptions>,
        /// If `None`, non found categories are not allowed
        fallback: Option<PngValidatorOptions>,
    },
    Ogg,
    Txt,
}

#[derive(Debug, Clone)]
pub enum AllowedResources {
    File(AllowedResource),
    Tar(Vec<AllowedResource>),
}

pub(crate) fn verify_resource(
    file_ty: &str,
    file_name: &str,
    file: &[u8],
    category: &Option<String>,
    allowed_resources: &[AllowedResources],
) -> AssetUploadResponse {
    match file_ty {
        "png" => {
            let Some(limits) = allowed_resources.iter().find_map(|r| {
                if let AllowedResources::File(AllowedResource::Png(limits)) = r {
                    Some(Some(*limits))
                } else if let AllowedResources::File(AllowedResource::PngCategory {
                    per_category,
                    fallback,
                }) = r
                {
                    Some(
                        category
                            .as_ref()
                            .and_then(|c| per_category.get(c).copied().or(*fallback)),
                    )
                } else {
                    None
                }
            }) else {
                return AssetUploadResponse::UnsupportedFileType;
            };
            let Some(limits) = limits else {
                return AssetUploadResponse::InvalidCategory;
            };
            if let Err(err) = is_png_image_valid(file, limits) {
                return AssetUploadResponse::BrokenFile(err.to_string());
            }
        }
        "ogg" => {
            if !allowed_resources
                .iter()
                .any(|r| matches!(r, AllowedResources::File(AllowedResource::Ogg)))
            {
                return AssetUploadResponse::UnsupportedFileType;
            };
            if let Err(err) = verify_ogg_vorbis(file) {
                return AssetUploadResponse::BrokenFile(err.to_string());
            }
        }
        "txt" => {
            if !allowed_resources
                .iter()
                .any(|r| matches!(r, AllowedResources::File(AllowedResource::Txt)))
            {
                return AssetUploadResponse::UnsupportedFileType;
            };
            match verify_txt(file, file_name) {
                Ok(_) => {}
                Err(err) => {
                    return AssetUploadResponse::BrokenFile(err.to_string());
                }
            }
        }
        "tar" => {
            let Some(allowed_resources) = allowed_resources.iter().find_map(|r| {
                if let AllowedResources::Tar(res) = r {
                    Some(res)
                } else {
                    None
                }
            }) else {
                return AssetUploadResponse::UnsupportedFileType;
            };
            match read_tar_files(file.into()) {
                Ok(files) => {
                    let allowed_resources: Vec<_> = allowed_resources
                        .iter()
                        .cloned()
                        .map(AllowedResources::File)
                        .collect();
                    let mut verified = true;
                    for (name, file) in &files {
                        let verify_res = verify_resource(
                            name.extension().and_then(|s| s.to_str()).unwrap_or(""),
                            name.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
                            file,
                            category,
                            &allowed_resources,
                        );
                        if !matches!(verify_res, AssetUploadResponse::Success) {
                            verified = false;
                            break;
                        }
                    }
                    if !verified {
                        return AssetUploadResponse::BrokenFile(
                            "tar contained invalid files".to_string(),
                        );
                    }
                }
                Err(err) => {
                    return AssetUploadResponse::BrokenFile(err.to_string());
                }
            }
        }
        _ => {
            return AssetUploadResponse::UnsupportedFileType;
        }
    }
    AssetUploadResponse::Success
}
