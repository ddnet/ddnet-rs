use std::sync::Arc;

use base_io::{io::IoFileSys, runtime::IoRuntimeTask};
pub use egui::FontDefinitions;
use egui::{FontData, FontFamily};
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

/// Loading process of shared font data
pub struct UiFontDataLoading {
    task: IoRuntimeTask<UiFontData>,
}

impl UiFontDataLoading {
    pub fn new(io: &IoFileSys) -> Self {
        let fs = io.fs.clone();
        let task = io.rt.spawn(async move {
            let mut icon = fs.read_file("fonts/Icons.otf".as_ref()).await?;
            let mut latin = fs.read_file("fonts/DejaVuSans.ttf".as_ref()).await?;
            let mut asia = fs
                .read_file("fonts/SourceHanSansSC-Regular.otf".as_ref())
                .await?;
            let mut mono = fs
                .read_file("fonts/SourceHanMono-Regular.otf".as_ref())
                .await?;

            icon.shrink_to_fit();
            latin.shrink_to_fit();
            asia.shrink_to_fit();
            mono.shrink_to_fit();

            Ok(UiFontData {
                icon,
                latin,
                asia,
                mono,
            })
        });

        Self { task }
    }
}

/// Font data that can (and maybe should) be shared
/// across multiple ui instances over your the application
#[derive(Debug, Hiarc, Serialize, Deserialize)]
pub struct UiFontData {
    pub icon: Vec<u8>,
    pub latin: Vec<u8>,
    pub asia: Vec<u8>,
    pub mono: Vec<u8>,
}

impl UiFontData {
    pub fn new(loading: UiFontDataLoading) -> anyhow::Result<Self> {
        loading.task.get()
    }

    pub fn into_font_definitions(self) -> FontDefinitions {
        let mut icon = self.icon;
        let mut latin = self.latin;
        let mut asia = self.asia;
        let mut mono = self.mono;

        icon.shrink_to_fit();
        latin.shrink_to_fit();
        asia.shrink_to_fit();
        mono.shrink_to_fit();

        let mut fonts = FontDefinitions::empty();
        fonts.font_data.insert(
            "default_latin".to_owned(),
            Arc::new(FontData::from_owned(latin)),
        );
        fonts.font_data.insert(
            "default_asia".to_owned(),
            Arc::new(FontData::from_owned(asia)),
        );
        fonts
            .font_data
            .insert("icons".to_owned(), Arc::new(FontData::from_owned(icon)));
        fonts
            .font_data
            .insert("mono".to_owned(), Arc::new(FontData::from_owned(mono)));

        // set font hierarchy
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "default_latin".to_owned());

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(1, "default_asia".to_owned());

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(2, "icons".to_owned());

        fonts
            .families
            .get_mut(&FontFamily::Monospace)
            .unwrap()
            .insert(0, "mono".to_owned());

        fonts
    }
}
