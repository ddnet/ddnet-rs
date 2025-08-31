use std::collections::HashMap;

use config::config::ConfigPath;
use config::{config::ConfigEngine, types::ConfRgb};
use config::{config_default, ConfigInterface};
use game_interface::interface::MAX_MAP_NAME_LEN;
use game_interface::{
    client_commands::MAX_TEAM_NAME_LEN,
    types::character_info::{
        NetworkLaserInfo, NetworkSkinInfo, MAX_ASSET_NAME_LEN, MAX_CHARACTER_CLAN_LEN,
        MAX_CHARACTER_NAME_LEN, MAX_FLAG_NAME_LEN, MAX_LANG_NAME_LEN,
    },
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, ConfigInterface, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum ConfigDummyScreenAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[config_default]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigDummy {
    /// Show the dummy in a miniscreen
    #[default = false]
    pub mini_screen: bool,
    /// The percentual width of the miniscreens (per anchor)
    #[conf_valid(range(min = 1, max = 100))]
    #[default = 40]
    pub screen_width: u32,
    /// The percentual height of the miniscreens (per anchor)
    #[conf_valid(range(min = 1, max = 100))]
    #[default = 40]
    pub screen_height: u32,
    /// To where the mini screen is anchored.
    #[default = ConfigDummyScreenAnchor::TopRight]
    pub screen_anchor: ConfigDummyScreenAnchor,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigTeam {
    /// Sets a custom team name
    #[conf_valid(length(max = MAX_TEAM_NAME_LEN))]
    #[default = ""]
    pub name: String,
    /// The color of the team in the scoreboard
    #[default = Default::default()]
    pub color: ConfRgb,
}

#[config_default]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, ConfigInterface)]
pub struct NoiseFilterSettings {
    /// Attennuation in db
    #[conf_valid(range(min = -200.0, max = 200.0))]
    #[default = 100.0]
    pub attenuation: f64,
    /// Threshold in db before processing is considered.
    #[conf_valid(range(min = -200.0, max = 200.0))]
    #[default = -10.0]
    pub processing_threshold: f64,
}

#[config_default]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, ConfigInterface)]
pub struct ConfigSpatialChatNoiseGate {
    /// Threshold in db when to allow voice to come through the gate
    #[conf_valid(range(min = -200.0, max = 200.0))]
    #[default = -36.0]
    pub open_threshold: f64,
    /// Threshold in db when to close the gate after previously playing voice data.
    #[conf_valid(range(min = -200.0, max = 200.0))]
    #[default = -54.0]
    pub close_threshold: f64,
}

#[config_default]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, ConfigInterface)]
pub struct ConfigSpatialChatFilter {
    /// Whether to use a noise filter at all
    #[default = true]
    pub use_nf: bool,
    pub nf: NoiseFilterSettings,
    /// When to allow voice and when to close the gate
    /// when voice was previously played.
    pub noise_gate: ConfigSpatialChatNoiseGate,
    /// Microphone boost in db
    #[conf_valid(range(min = -200.0, max = 200.0))]
    pub boost: f64,
}

#[config_default]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, ConfigInterface)]
pub struct ConfigSpatialChatPerPlayerOptions {
    /// Is the player muted completely?
    pub muted: bool,
    /// Whether to force a noise filter for this player.
    /// Note that this is generally a very expensive operation
    /// and uses lot of RAM.
    pub force_nf: bool,
    pub nf: NoiseFilterSettings,
    /// Whether to force a noise gate for
    /// this player. Uses extra CPU time.
    pub force_gate: bool,
    pub noise_gate: ConfigSpatialChatNoiseGate,
    /// Boost of the user's sound in db
    #[conf_valid(range(min = -200.0, max = 200.0))]
    pub boost: f64,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ConfigInterface)]
pub struct ConfigSpatialChat {
    /// Helper about if the user read about risks
    /// of using spatial chat
    #[default = false]
    pub read_warning: bool,
    /// Whether spatial chat is allowed (sending microphone data)
    #[default = false]
    pub activated: bool,
    /// Use spatial sound (instead of mono that gets more silent).
    #[default = true]
    pub spatial: bool,
    /// The sound driver
    pub host: String,
    /// The sound card
    pub device: String,
    /// Filter settings for the microphone
    pub filter: ConfigSpatialChatFilter,
    /// Allow to play voice from users that
    /// don't have an account.
    #[default = false]
    pub from_non_account_users: bool,
    /// Users with an account that are permanentally muted. The key
    /// is the account id as string
    pub account_players: HashMap<String, ConfigSpatialChatPerPlayerOptions>,
    /// Users withour an account that are permanentally muted.
    /// The key is the hash formatted as string
    pub account_certs: HashMap<String, ConfigSpatialChatPerPlayerOptions>,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigDemoRecorder {
    /// How many frames per second the video should have
    #[default = 60]
    pub fps: u32,
    /// How many pixels per logical unit there are.
    /// Higher values make UI elements bigger.
    #[conf_valid(range(min = -20.0, max = 20.0))]
    #[default = 1.0]
    pub pixels_per_point: f64,
    /// The width of the video
    #[default = 1920]
    pub width: u32,
    /// The height of the video
    #[default = 1080]
    pub height: u32,
    /// Use hw accel
    #[default = ""]
    pub hw_accel: String,
    /// The sample rate for the audio stream.
    /// Should be a multiple of `fps` for best results.
    #[default = 48000]
    pub sample_rate: u32,
    /// "Constant Rate Factor" for x264.
    /// Where 0 is lossless and 51 is the worst.
    /// 18 is default.
    #[default = 18]
    pub crf: u8,
    /// Config related to rendering graphics & sound.
    pub render: ConfigRender,
    /// Sound configs used during rendering sound & graphics.
    pub snd: ConfigSoundRender,
    #[conf_valid(range(min = 0.0, max = 1.0))]
    #[default = 0.3]
    /// The overall volume for all sounds (applied as multiplier).
    pub global_sound_volume: f64,
}

/// Config related to rendering graphics & sound.
#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigRender {
    /// Whether to show nameplates.
    #[default = true]
    pub nameplates: bool,
    /// Whether to show the nameplate of the own character.
    #[default = false]
    pub own_nameplate: bool,
    /// How much to scale ingame menus
    #[conf_valid(range(min = 0.1, max = 5.0))]
    #[default = 1.0]
    pub ingame_ui_scale: f64,
    /// How much pixels per point (similar to DPI) to at least assume.
    #[conf_valid(range(min = 0.1, max = 5.0))]
    #[default = 1.5]
    pub ingame_ui_min_pixels_per_point: f64,
    /// How transparent are characters in solo ddrace parts or from other ddrace teams.
    #[conf_valid(range(min = 0.0, max = 1.0))]
    #[default = 0.5]
    pub phased_alpha: f64,
    /// Whether hook related sounds should happen where the hook
    /// is instead of where the character that owns the hook is.
    /// In teeworlds it's the latter.
    #[default = false]
    pub hook_sound_on_hook_pos: bool,
    /// If this is enabled, then the ingame aspect ratio is used
    /// for rendering of ingame components (map, players, projectiles etc.)
    pub use_ingame_aspect_ratio: bool,
    /// If ingame aspect ratio is enabled, this is the ratio used
    /// during rendering of ingame components (map, players, projectiles etc.)
    #[conf_valid(range(min = 0.5, max = 3.0))]
    #[default = (16.0 / 9.0)]
    pub ingame_aspect_ratio: f64,
    /// Whether to enable dynamic camera while spectating another
    /// character.
    #[default = false]
    pub spec_dyncam: bool,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigClient {
    /// The client started the first time ever and should do
    /// initial setup and give the new user an introduction.
    #[default = true]
    pub first_time_setup: bool,
    /// Show a fps counter.
    #[default = false]
    pub show_fps: bool,
    /// How often the game loop should run per second.
    #[default = 0]
    pub refresh_rate: u64,
    /// Dummy related settings.
    #[default = Default::default()]
    pub dummy: ConfigDummy,
    /// DDrace-Team related settings.
    pub team: ConfigTeam,
    /// Config related to rendering graphics & sound.
    pub render: ConfigRender,
    #[default = "autumn"]
    pub menu_background_map: String,
    /// Configs related to spatial chat support.
    pub spatial_chat: ConfigSpatialChat,
    /// Configurations for the demo video encoder.
    pub recorder: ConfigDemoRecorder,
    /// Apply input for prediction directly. Might cause miss prediction.
    pub instant_input: bool,
    /// Predict other entities that are not local as if the ping is 0.
    pub anti_ping: bool,
    /// The rendering mod to use, whenever possible.
    /// Empty string, "default", "native", "vanilla" & "ddnet"
    /// are reserved names and won't cause any mod to load.
    #[default = ""]
    pub render_mod: String,
    #[conf_valid(length(max = MAX_LANG_NAME_LEN))]
    #[default = "en"]
    pub language: String,
    /// Whether spectating over the ingame spectator
    /// list should make the character be phased.
    pub phased_ingame_spectate: bool,
    /// Whether the client showed a warning when joining a legacy
    /// server and should not show it again.
    pub shown_legacy_server_warning: bool,
    #[default = true]
    /// Enables the auto update if available
    pub auto_updater: bool,
}

#[config_default]
#[derive(Debug, Serialize, Deserialize, ConfigInterface, Clone)]
pub struct ConfigPlayerSkin {
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub name: String,
    #[default = Default::default()]
    pub body_color: ConfRgb,
    #[default = Default::default()]
    pub feet_color: ConfRgb,
    /// Use the custom/user-defined colors for the skin
    #[default = false]
    pub custom_colors: bool,
}

impl From<&ConfigPlayerSkin> for NetworkSkinInfo {
    fn from(value: &ConfigPlayerSkin) -> Self {
        if value.custom_colors {
            Self::Custom {
                body_color: value.body_color.into(),
                feet_color: value.feet_color.into(),
            }
        } else {
            Self::Original
        }
    }
}

#[config_default]
#[derive(Debug, Serialize, Deserialize, ConfigInterface, Clone)]
pub struct ConfigPlayerLaser {
    #[default = Default::default()]
    pub inner_color: ConfRgb,
    #[default = Default::default()]
    pub outer_color: ConfRgb,
}

impl From<&ConfigPlayerLaser> for NetworkLaserInfo {
    fn from(value: &ConfigPlayerLaser) -> Self {
        Self {
            inner_color: value.inner_color.into(),
            outer_color: value.outer_color.into(),
        }
    }
}

#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    ConfigInterface,
)]
pub enum ConfigTeeEye {
    #[default]
    Normal = 0,
    Pain,
    Happy,
    Surprised,
    Angry,
    Blink,
}

#[config_default]
#[derive(Debug, Serialize, Deserialize, ConfigInterface, Clone)]
pub struct ConfigPlayer {
    #[conf_valid(length(max = MAX_CHARACTER_NAME_LEN))]
    #[default = "nameless tee"]
    pub name: String,
    #[conf_valid(length(max = MAX_CHARACTER_CLAN_LEN))]
    #[default = ""]
    pub clan: String,

    #[default = Default::default()]
    pub skin: ConfigPlayerSkin,

    pub laser: ConfigPlayerLaser,

    #[conf_valid(length(max = MAX_FLAG_NAME_LEN))]
    #[default = "default"]
    pub flag: String,
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub weapon: String,
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub freeze: String,
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub ninja: String,
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub game: String,
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub ctf: String,
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub hud: String,
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub entities: String,
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub emoticons: String,
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub particles: String,
    #[conf_valid(length(max = MAX_ASSET_NAME_LEN))]
    #[default = "default"]
    pub hook: String,
    #[default = Vec::new()]
    pub binds: Vec<String>,
    /// The default eyes to use if the server supports custom eyes.
    #[default = ConfigTeeEye::Normal]
    pub eyes: ConfigTeeEye,
}

impl ConfigPlayer {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }
}

#[config_default]
#[derive(Debug, Serialize, Deserialize, ConfigInterface, Clone)]
pub struct ConfigDummyProfile {
    /// An index for an array of [`ConfigPlayer`].
    #[default = 1]
    pub index: u64,
    /// Whether to copy assets from the main player's profile.
    #[default = true]
    pub copy_assets_from_main: bool,
    /// Whether to copy binds from the main player's profile.
    #[default = true]
    pub copy_binds_from_main: bool,
}

#[config_default]
#[derive(Debug, Serialize, Deserialize, ConfigInterface, Clone)]
pub struct ConfigPlayerProfiles {
    /// The main player. An index for an array of [`ConfigPlayer`].
    pub main: u64,
    /// The dummy of the main player.
    pub dummy: ConfigDummyProfile,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigMap {
    #[default = true]
    pub high_detail: bool,
    #[default = true]
    pub background_show_tile_layers: bool,
    #[default = true]
    pub show_quads: bool,
    #[conf_valid(range(min = 0, max = 100))]
    #[default = 0]
    pub physics_layer_opacity: u8,
    #[default = true]
    pub text_entities: bool,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigMouse {
    /// The sensitivity of the mouse.
    #[default = 100.0]
    #[conf_valid(range(min = 0.0, max = 100000.0))]
    pub sensitivity: f64,
    /// The minimal distance from own character to the cursor.
    #[default = 0.0]
    #[conf_valid(range(min = 0.0, max = 100000.0))]
    pub min_distance: f64,
    /// The maximal distance from own character to the cursor.
    #[default = 400.0]
    #[conf_valid(range(min = 0.0, max = 100000.0))]
    pub max_distance: f64,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigDyncamMouse {
    /// The sensitivity of the mouse.
    #[default = 100.0]
    #[conf_valid(range(min = 0.0, max = 100000.0))]
    pub sensitivity: f64,
    /// Factor of how much the camera should follow the mouse.
    #[conf_valid(range(min = 0.0, max = 200.0))]
    #[default = 60.0]
    pub follow_factor: f64,
    /// A deadzone in which the mouse should not follow the mouse.
    #[default = 300.0]
    #[conf_valid(range(min = 0.0, max = 100000.0))]
    pub deadzone: f64,
    /// The minimal distance from own character to the cursor.
    #[default = 0.0]
    #[conf_valid(range(min = 0.0, max = 100000.0))]
    pub min_distance: f64,
    /// The maximal distance from own character to the cursor.
    #[default = 400.0]
    #[conf_valid(range(min = 0.0, max = 100000.0))]
    pub max_distance: f64,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigInput {
    /// Settings related to the mouse.
    pub mouse: ConfigMouse,
    /// Settings related to the mouse of a dynamic camera.
    pub dyncam_mouse: ConfigDyncamMouse,
    /// Whether to use the dynamic camera mouse.
    pub use_dyncam: bool,
}

impl ConfigInput {
    pub fn follow_factor_or_zero(&self) -> f64 {
        if self.use_dyncam {
            self.dyncam_mouse.follow_factor
        } else {
            0.0
        }
    }
    pub fn deadzone_or_zero(&self) -> f64 {
        if self.use_dyncam {
            self.dyncam_mouse.deadzone
        } else {
            0.0
        }
    }
    pub fn min_distance(&self) -> f64 {
        if self.use_dyncam {
            self.dyncam_mouse.min_distance
        } else {
            self.mouse.min_distance
        }
    }
    pub fn max_distance(&self) -> f64 {
        if self.use_dyncam {
            self.dyncam_mouse.max_distance
        } else {
            self.mouse.max_distance
        }
    }
    pub fn sensitivity(&self) -> f64 {
        if self.use_dyncam {
            self.dyncam_mouse.sensitivity
        } else {
            self.mouse.sensitivity
        }
    }
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigServerDatabaseConnection {
    #[default = ""]
    pub username: String,
    #[default = ""]
    pub password: String,
    /// The database name.
    /// For sqlite this is the sqlite file.
    #[default = ""]
    pub database: String,
    #[default = "127.0.0.1"]
    pub host: String,
    #[default = 3306]
    pub port: u16,
    /// Server certificate that the client trusts.
    /// Can be ignored for localhost & sqlite.
    #[default = ""]
    pub ca_cert_path: String,
    #[default = 64]
    pub connection_count: u64,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigServerDatabase {
    /// Connections to a database.
    /// The key value here is the type of databse (mysql, sqlite).
    /// Additionally the key allows `_backup` as suffix to connect to a backup database.
    pub connections: HashMap<String, ConfigServerDatabaseConnection>,
    /// Specify the database type where accounts will be enabled.
    /// Only one database type is allowed and must be enabled in the connections.
    #[default = ""]
    pub enable_accounts: String,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigServerRenderMod {
    #[default = ""]
    pub name: String,
    pub hash: Vec<u8>,
    /// Whether the game __has__ to load the module in order
    /// to play on this server.
    pub required: bool,
}

pub const MAX_SERVER_NAME_LEN: usize = 64;
#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigServer {
    #[conf_valid(length(max = MAX_SERVER_NAME_LEN))]
    #[default = "unnamed server"]
    pub name: String,
    #[conf_valid(length(max = MAX_MAP_NAME_LEN))]
    #[default = "ctf1"]
    pub map: String,
    #[default = "0.0.0.0"]
    pub bind_addr_v4: String,
    #[default = "::0"]
    pub bind_addr_v6: String,
    #[default = 8310]
    pub port_v4: u16,
    #[default = 8311]
    pub port_v6: u16,
    /// The ipv4 port to use for the resource download server
    #[default = 0]
    pub download_server_port_v4: u16,
    /// The ipv6 port to use for the resource download server
    #[default = 0]
    pub download_server_port_v6: u16,
    /// port for the internal server (inside the client)
    /// defaults to 0 -> random port
    #[default = 0]
    pub port_internal: u16,
    /// The maximum allowed number of connections
    /// to this server
    #[default = 128]
    #[conf_valid(range(min = 1, max = 1000000))]
    pub max_connections: u32,
    /// The maximum allowed connections per ip
    #[default = 4]
    #[conf_valid(range(min = 1, max = 1000000))]
    pub max_connections_per_ip: u32,
    /// The maximum allowed players
    /// from all clients (includes all dummies)
    /// combined concurrently
    /// playing or spectating in the server.
    #[default = 64]
    #[conf_valid(range(min = 1, max = 1000000))]
    pub max_players: u32,
    /// How many dummies a player can connect.
    /// This includes the main player and thus
    /// must be at least 1.
    ///
    /// (Or split screen players)
    #[default = 2]
    #[conf_valid(range(min = 1, max = 1000000))]
    pub max_players_per_client: u32,
    /// Only clients with a valid account can connect.
    /// This is only active if accounts were enabled
    /// in the database configuration.
    #[default = false]
    pub account_only: bool,
    #[default = false]
    pub register: bool,
    /// The game mod module to load
    /// empty string, "default", "native", "vanilla" & "ddnet"
    /// are reserved names and will not cause
    /// loading a game mod module
    #[default = ""]
    pub game_mod: String,
    /// The render mod module, that the client should load.
    /// Empty string, "default", "native", "vanilla" & "ddnet"
    /// are reserved names and will not cause
    /// loading a render mod module.
    pub render_mod: ConfigServerRenderMod,
    #[default = Default::default()]
    /// The database configuration.
    /// They should be used if the mod requires database support.
    /// Databases are generally optional.
    pub db: ConfigServerDatabase,
    /// How many ticks must pass before sending the next snapshot
    #[conf_valid(range(min = 1, max = 100))]
    #[default = 2]
    pub ticks_per_snapshot: u64,
    /// Train a packet dictionary. (for compression)
    /// Don't activate this if you don't know what this means
    #[default = false]
    pub train_packet_dictionary: bool,
    #[conf_valid(range(min = 256, max = 104857600))]
    #[default = 65536]
    pub train_packet_dictionary_max_size: u32,
    /// Automatically make all maps that can be found
    /// as map vote. (Usually only recommended for test servers
    /// and local servers).
    #[default = false]
    pub auto_map_votes: bool,
    /// Path to the map votes file.
    #[default = "map_votes.json"]
    pub map_votes_path: String,
    /// Path to the server provided asset files.
    /// The dictionary structure should match the one from
    /// the data directory.
    /// Assets include the hash in the file name.
    /// An empty string disables server provided assets.
    #[default = ""]
    pub provided_assets_path: String,
    /// Whether to allow spatial chat on this server.
    /// Note that spatial chat causes lot of network
    /// traffic.
    #[default = false]
    pub spatial_chat: bool,
    /// If set, client's must input the correct password
    /// before being able to join the server
    #[default = ""]
    pub password: String,
}

/// Sound configs used during rendering sound & graphics.
#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigSoundRender {
    /// Use spatial emitters for ingame sounds.
    #[default = false]
    pub spatial: bool,
    /// The sound volume for ingame sounds
    #[conf_valid(range(min = 0.0, max = 1.0))]
    #[default = 1.0]
    pub ingame_sound_volume: f64,
    /// The sound volume for map music/sounds
    #[conf_valid(range(min = 0.0, max = 1.0))]
    #[default = 1.0]
    pub map_sound_volume: f64,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigSound {
    /// Sound configs used during rendering sound & graphics.
    pub render: ConfigSoundRender,
    /// The overall volume multiplier
    #[conf_valid(range(min = 0.0, max = 1.0))]
    #[default = 0.3]
    pub global_volume: f64,
}

#[config_default]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigDebugClientServerSyncLog {
    /// only works without ping jitter
    #[default = false]
    pub time: bool,
    #[default = false]
    pub inputs: bool,
    #[default = false]
    pub players: bool,
    #[default = false]
    pub projectiles: bool,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigDebug {
    /// Log some sync related stuff from the internal server & client
    /// only use in release mode
    pub client_server_sync_log: ConfigDebugClientServerSyncLog,
}

#[config_default]
#[derive(Debug, Clone, Serialize, Deserialize, ConfigInterface)]
pub struct ConfigGame {
    // Client related config.
    pub cl: ConfigClient,
    // List of players' config.
    #[conf_valid(length(min = 2))]
    #[default = vec![ConfigPlayer::default(), ConfigPlayer::new("brainless tee")]]
    #[conf_alias(player, players[$profiles.main$])]
    #[conf_alias(dummy, players[$profiles.dummy.index$])]
    pub players: Vec<ConfigPlayer>,
    pub profiles: ConfigPlayerProfiles,
    // Map rendering related config.
    pub map: ConfigMap,
    // Input related config.
    pub inp: ConfigInput,
    // Server related config.
    pub sv: ConfigServer,
    // Sound related config.
    pub snd: ConfigSound,
    // Debug related config for the game.
    pub dbg: ConfigDebug,
}

impl ConfigGame {
    pub fn new() -> ConfigGame {
        Self::default()
    }

    pub fn to_json_string(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json_string(json_str: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(json_str)?)
    }

    pub fn from_json_slice(json: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(json)?)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub game: ConfigGame,
    pub engine: ConfigEngine,
}

impl Config {
    pub fn new(game: ConfigGame, engine: ConfigEngine) -> Config {
        Config { game, engine }
    }

    /// Shortcut for ui storage
    pub fn storage_opt<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.engine
            .ui
            .storage
            .get(key)
            .and_then(|str| serde_json::from_str(str).ok())
            .unwrap_or_default()
    }

    /// Shortcut for ui storage
    pub fn storage<T: Default + DeserializeOwned>(&self, key: &str) -> T {
        self.storage_opt(key).unwrap_or_default()
    }

    /// Shortcut for ui storage
    pub fn set_storage<T: Serialize>(&mut self, key: &str, data: &T) {
        self.engine
            .ui
            .storage
            .insert(key.to_string(), serde_json::to_string(&data).unwrap());
    }

    /// Shortcut for ui storage
    pub fn rem_storage(&mut self, key: &str) {
        self.engine.ui.storage.remove(key);
    }

    /// Shortcut for ui storage
    pub fn storage_entry(&mut self, key: &str) -> &mut String {
        self.engine.ui.storage.entry(key.to_string()).or_default()
    }

    /// Shortcut for ui path
    pub fn path(&mut self) -> &mut ConfigPath {
        &mut self.engine.ui.path
    }
}
