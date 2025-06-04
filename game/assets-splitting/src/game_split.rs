use anyhow::anyhow;

#[derive(Debug, Clone)]
pub struct Game06Part {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Game06Part {
    fn new(data: Vec<u8>, width: usize, height: usize) -> Self {
        Self {
            data,
            width: width as u32,
            height: height as u32,
        }
    }

    fn if_not_fully_transparent(self) -> Option<Self> {
        self.data
            .chunks_exact(4)
            .any(|pixels| pixels[3] != 0)
            .then_some(self)
    }
}

#[derive(Debug)]
pub struct Game06ConvertResult {
    pub cursor_hammer: Game06Part,
    pub cursor_gun: Game06Part,
    pub cursor_shotgun: Game06Part,
    pub cursor_grenade: Game06Part,
    pub cursor_ninja: Game06Part,
    pub cursor_laser: Game06Part,

    pub weapon_hammer: Game06Part,
    pub weapon_gun: Game06Part,
    pub weapon_shotgun: Game06Part,
    pub weapon_grenade: Game06Part,
    pub weapon_ninja: Game06Part,
    pub weapon_laser: Game06Part,

    pub projectile_gun: Game06Part,
    pub projectile_shotgun: Game06Part,
    pub projectile_grenade: Game06Part,
    pub projectile_laser: Game06Part,

    pub muzzle_gun: [Game06Part; 3],
    pub muzzle_shotgun: [Game06Part; 3],
    pub muzzle_ninja: [Game06Part; 3],

    pub flag_blue: Game06Part,
    pub flag_red: Game06Part,

    pub hook_chain: Game06Part,
    pub hook_head: Game06Part,

    pub star1: Game06Part,
    pub star2: Game06Part,
    pub star3: Game06Part,

    pub health_full: Game06Part,
    pub health_empty: Game06Part,

    pub armor_full: Game06Part,
    pub armor_empty: Game06Part,

    // "weapon shields"
    pub lose_shotgun: Option<Game06Part>,
    pub lose_grenade: Option<Game06Part>,
    pub lose_laser: Option<Game06Part>,
    pub lose_ninja: Option<Game06Part>,

    pub particles: [Game06Part; 9],

    pub pickup_health: Game06Part,
    pub pickup_armor: Game06Part,

    pub ninja_bar_full_left: Option<Game06Part>,
    pub ninja_bar_full: Option<Game06Part>,
    pub ninja_bar_empty: Option<Game06Part>,
    pub ninja_bar_empty_right: Option<Game06Part>,
}

fn single_img(
    game_file: &[u8],
    x: usize,
    y: usize,
    sub_width: usize,
    sub_height: usize,
    pitch: usize,
) -> Game06Part {
    let mut res: Vec<u8> = Default::default();

    let in_line = game_file
        .split_at(y * pitch)
        .1
        .split_at(sub_height * pitch)
        .0;
    in_line.chunks(pitch).for_each(|chunk| {
        res.extend(chunk.split_at(x * 4).1.split_at(sub_width * 4).0);
    });

    Game06Part::new(res, sub_width, sub_height)
}

/// splits the game.png into its individual components
/// Additionally the width has to be divisible by 32
/// and the height by 16
pub fn split_06_game(
    game_file: &[u8],
    width: u32,
    height: u32,
) -> anyhow::Result<Game06ConvertResult> {
    if width % 32 != 0 {
        Err(anyhow!("width is not divisible by 32"))
    } else if height % 16 != 0 {
        Err(anyhow!("height is not divisible by 16"))
    } else {
        let full_width = width as usize * 4; // * 4 for RGBA
        let segment_width = width as usize / 32;
        let segment_height = height as usize / 16;

        let health_full = single_img(
            game_file,
            21 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let health_empty = single_img(
            game_file,
            23 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let armor_full = single_img(
            game_file,
            21 * segment_width,
            2 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let armor_empty = single_img(
            game_file,
            23 * segment_width,
            2 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        let star1 = single_img(
            game_file,
            15 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let star2 = single_img(
            game_file,
            17 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let star3 = single_img(
            game_file,
            19 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        let lose_shotgun = single_img(
            game_file,
            15 * segment_width,
            2 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let lose_grenade = single_img(
            game_file,
            17 * segment_width,
            2 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let lose_laser = single_img(
            game_file,
            19 * segment_width,
            2 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        let part1 = single_img(
            game_file,
            6 * segment_width,
            0 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part2 = single_img(
            game_file,
            6 * segment_width,
            1 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part3 = single_img(
            game_file,
            7 * segment_width,
            0 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part4 = single_img(
            game_file,
            7 * segment_width,
            1 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part5 = single_img(
            game_file,
            8 * segment_width,
            0 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part6 = single_img(
            game_file,
            8 * segment_width,
            1 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part7 = single_img(
            game_file,
            9 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let part8 = single_img(
            game_file,
            11 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let part9 = single_img(
            game_file,
            13 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        let weapon_gun = single_img(
            game_file,
            2 * segment_width,
            4 * segment_height,
            4 * segment_width,
            2 * segment_height,
            full_width,
        );
        let cursor_gun = single_img(
            game_file,
            0 * segment_width,
            4 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let projectile_gun = single_img(
            game_file,
            6 * segment_width,
            4 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let weapon_gun_muzzle1 = single_img(
            game_file,
            8 * segment_width,
            4 * segment_height,
            4 * segment_width,
            2 * segment_height,
            full_width,
        );
        let weapon_gun_muzzle2 = single_img(
            game_file,
            12 * segment_width,
            4 * segment_height,
            4 * segment_width,
            2 * segment_height,
            full_width,
        );
        let weapon_gun_muzzle3 = single_img(
            game_file,
            16 * segment_width,
            4 * segment_height,
            4 * segment_width,
            2 * segment_height,
            full_width,
        );

        let weapon_shotgun = single_img(
            game_file,
            2 * segment_width,
            6 * segment_height,
            8 * segment_width,
            2 * segment_height,
            full_width,
        );
        let cursor_shotgun = single_img(
            game_file,
            0 * segment_width,
            6 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let projectile_shotgun = single_img(
            game_file,
            10 * segment_width,
            6 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let weapon_shotgun_muzzle1 = single_img(
            game_file,
            12 * segment_width,
            6 * segment_height,
            4 * segment_width,
            2 * segment_height,
            full_width,
        );
        let weapon_shotgun_muzzle2 = single_img(
            game_file,
            16 * segment_width,
            6 * segment_height,
            4 * segment_width,
            2 * segment_height,
            full_width,
        );
        let weapon_shotgun_muzzle3 = single_img(
            game_file,
            20 * segment_width,
            6 * segment_height,
            4 * segment_width,
            2 * segment_height,
            full_width,
        );

        let weapon_grenade = single_img(
            game_file,
            2 * segment_width,
            8 * segment_height,
            7 * segment_width,
            2 * segment_height,
            full_width,
        );
        let cursor_grenade = single_img(
            game_file,
            0 * segment_width,
            8 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let projectile_grenade = single_img(
            game_file,
            10 * segment_width,
            8 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        let lose_ninja = single_img(
            game_file,
            10 * segment_width,
            10 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        let weapon_hammer = single_img(
            game_file,
            2 * segment_width,
            1 * segment_height,
            4 * segment_width,
            3 * segment_height,
            full_width,
        );
        let cursor_hammer = single_img(
            game_file,
            0 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        let weapon_ninja = single_img(
            game_file,
            2 * segment_width,
            10 * segment_height,
            8 * segment_width,
            2 * segment_height,
            full_width,
        );
        let cursor_ninja = single_img(
            game_file,
            0 * segment_width,
            10 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        let weapon_laser = single_img(
            game_file,
            2 * segment_width,
            12 * segment_height,
            7 * segment_width,
            3 * segment_height,
            full_width,
        );
        let cursor_laser = single_img(
            game_file,
            0 * segment_width,
            12 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let projectile_laser = single_img(
            game_file,
            10 * segment_width,
            12 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        let hook_chain = single_img(
            game_file,
            2 * segment_width,
            0 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let hook_head = single_img(
            game_file,
            3 * segment_width,
            0 * segment_height,
            2 * segment_width,
            1 * segment_height,
            full_width,
        );

        let weapon_ninja_muzzle1 = single_img(
            game_file,
            25 * segment_width,
            0 * segment_height,
            7 * segment_width,
            4 * segment_height,
            full_width,
        );
        let weapon_ninja_muzzle2 = single_img(
            game_file,
            25 * segment_width,
            4 * segment_height,
            7 * segment_width,
            4 * segment_height,
            full_width,
        );
        let weapon_ninja_muzzle3 = single_img(
            game_file,
            25 * segment_width,
            8 * segment_height,
            7 * segment_width,
            4 * segment_height,
            full_width,
        );

        let pickup_health = single_img(
            game_file,
            10 * segment_width,
            2 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let pickup_armor = single_img(
            game_file,
            12 * segment_width,
            2 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        let flag_blue = single_img(
            game_file,
            12 * segment_width,
            8 * segment_height,
            4 * segment_width,
            8 * segment_height,
            full_width,
        );
        let flag_red = single_img(
            game_file,
            16 * segment_width,
            8 * segment_height,
            4 * segment_width,
            8 * segment_height,
            full_width,
        );

        let ninja_bar_full_left = single_img(
            game_file,
            21 * segment_width,
            4 * segment_height,
            1 * segment_width,
            2 * segment_height,
            full_width,
        );
        let ninja_bar_full = single_img(
            game_file,
            22 * segment_width,
            4 * segment_height,
            1 * segment_width,
            2 * segment_height,
            full_width,
        );
        let ninja_bar_empty = single_img(
            game_file,
            23 * segment_width,
            4 * segment_height,
            1 * segment_width,
            2 * segment_height,
            full_width,
        );
        let ninja_bar_empty_right = single_img(
            game_file,
            24 * segment_width,
            4 * segment_height,
            1 * segment_width,
            2 * segment_height,
            full_width,
        );

        Ok(Game06ConvertResult {
            cursor_hammer,
            cursor_gun,
            cursor_shotgun,
            cursor_grenade,
            cursor_ninja,
            cursor_laser,

            weapon_hammer,
            weapon_gun,
            weapon_shotgun,
            weapon_grenade,
            weapon_ninja,
            weapon_laser,

            projectile_gun,
            projectile_shotgun,
            projectile_grenade,
            projectile_laser,

            muzzle_gun: [weapon_gun_muzzle1, weapon_gun_muzzle2, weapon_gun_muzzle3],
            muzzle_shotgun: [
                weapon_shotgun_muzzle1,
                weapon_shotgun_muzzle2,
                weapon_shotgun_muzzle3,
            ],
            muzzle_ninja: [
                weapon_ninja_muzzle1,
                weapon_ninja_muzzle2,
                weapon_ninja_muzzle3,
            ],

            flag_blue,
            flag_red,

            hook_chain,
            hook_head,

            star1,
            star2,
            star3,

            health_full,
            health_empty,

            armor_full,
            armor_empty,

            particles: [
                part1, part2, part3, part4, part5, part6, part7, part8, part9,
            ],

            pickup_health,
            pickup_armor,

            lose_grenade: lose_grenade.if_not_fully_transparent(),
            lose_laser: lose_laser.if_not_fully_transparent(),
            lose_ninja: lose_ninja.if_not_fully_transparent(),
            lose_shotgun: lose_shotgun.if_not_fully_transparent(),

            ninja_bar_full_left: ninja_bar_full_left.if_not_fully_transparent(),
            ninja_bar_full: ninja_bar_full.if_not_fully_transparent(),
            ninja_bar_empty: ninja_bar_empty.if_not_fully_transparent(),
            ninja_bar_empty_right: ninja_bar_empty_right.if_not_fully_transparent(),
        })
    }
}
