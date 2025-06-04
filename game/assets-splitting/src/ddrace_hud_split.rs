use anyhow::anyhow;

#[derive(Debug, Clone)]
pub struct DdraceHudPart {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl DdraceHudPart {
    fn new(data: Vec<u8>, width: usize, height: usize) -> Self {
        Self {
            data,
            width: width as u32,
            height: height as u32,
        }
    }
}

#[derive(Debug)]
pub struct DdraceHudConvertResult {
    pub jump: DdraceHudPart,
    pub jump_used: DdraceHudPart,
    pub solo: DdraceHudPart,
    pub collision_off: DdraceHudPart,
    pub endless_jump: DdraceHudPart,
    pub endless_hook: DdraceHudPart,
    pub jetpack: DdraceHudPart,

    pub freeze_left: DdraceHudPart,
    pub freeze_right: DdraceHudPart,
    pub disabled_hook_others: DdraceHudPart,
    pub disabled_hammer: DdraceHudPart,
    pub disabled_shotgun: DdraceHudPart,
    pub disabled_grenade: DdraceHudPart,
    pub disabled_laser: DdraceHudPart,
    pub disabled_gun: DdraceHudPart,

    pub ninja_left: DdraceHudPart,
    pub ninja_right: DdraceHudPart,
    pub tele_grenade: DdraceHudPart,
    pub tele_pistol: DdraceHudPart,
    pub tele_laser: DdraceHudPart,
    pub deep_frozen: DdraceHudPart,
    pub live_frozen: DdraceHudPart,

    pub disabled_finish: DdraceHudPart,
    pub dummy_hammer: DdraceHudPart,
    pub dummy_copy: DdraceHudPart,
    pub stage_locked: DdraceHudPart,
    pub team0_mode: DdraceHudPart,
}

/// splits the ddrace hud file into its (up to) 256 individual parts
/// Additionally the width & height have to be divisible by 16
pub fn split_ddrace_hud(
    file: &[u8],
    width: u32,
    height: u32,
) -> anyhow::Result<DdraceHudConvertResult> {
    if width % 16 != 0 {
        Err(anyhow!("width is not divisible by 16"))
    } else if height % 16 != 0 {
        Err(anyhow!("height is not divisible by 16"))
    } else {
        let mut jump: Vec<u8> = Default::default();
        let mut jump_used: Vec<u8> = Default::default();
        let mut solo: Vec<u8> = Default::default();
        let mut collision_off: Vec<u8> = Default::default();
        let mut endless_jump: Vec<u8> = Default::default();
        let mut endless_hook: Vec<u8> = Default::default();
        let mut jetpack: Vec<u8> = Default::default();

        let mut freeze_left: Vec<u8> = Default::default();
        let mut freeze_right: Vec<u8> = Default::default();
        let mut ninja_left: Vec<u8> = Default::default();
        let mut ninja_right: Vec<u8> = Default::default();
        let mut disabled_hook_others: Vec<u8> = Default::default();
        let mut disabled_hammer: Vec<u8> = Default::default();
        let mut disabled_shotgun: Vec<u8> = Default::default();
        let mut disabled_grenade: Vec<u8> = Default::default();
        let mut disabled_laser: Vec<u8> = Default::default();
        let mut disabled_gun: Vec<u8> = Default::default();

        let mut tele_grenade: Vec<u8> = Default::default();
        let mut tele_pistol: Vec<u8> = Default::default();
        let mut tele_laser: Vec<u8> = Default::default();
        let mut deep_frozen: Vec<u8> = Default::default();
        let mut live_frozen: Vec<u8> = Default::default();

        let mut disabled_finish: Vec<u8> = Default::default();
        let mut dummy_hammer: Vec<u8> = Default::default();
        let mut dummy_copy: Vec<u8> = Default::default();
        let mut stage_locked: Vec<u8> = Default::default();
        let mut team0_mode: Vec<u8> = Default::default();

        let full_width = width as usize * 4; // * 4 for RGBA
        let segment_width = width as usize / 16;
        let segment_full_width = segment_width * 4; // * 4 for RGBA
        let segment_height = height as usize / 16;
        file.chunks_exact(full_width)
            .enumerate()
            .for_each(|(y, y_chunk)| {
                if y < segment_height * 2 {
                    let rest = y_chunk;

                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    jump.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    jump_used.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    solo.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    collision_off.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    endless_jump.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    endless_hook.extend(img_part);
                    let (img_part, _) = rest.split_at(segment_full_width * 2);
                    jetpack.extend(img_part);
                } else if y < segment_height * 2 * 2 {
                    let rest = y_chunk;

                    let rest = if y < segment_height * 2 + segment_height {
                        let (img_part, rest) = rest.split_at(segment_full_width * 2);
                        freeze_left.extend(img_part);
                        let (img_part, rest) = rest.split_at(segment_full_width * 2);
                        freeze_right.extend(img_part);
                        rest
                    } else {
                        let (img_part, rest) = rest.split_at(segment_full_width * 2);
                        ninja_left.extend(img_part);
                        let (img_part, rest) = rest.split_at(segment_full_width * 2);
                        ninja_right.extend(img_part);
                        rest
                    };

                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    disabled_hook_others.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    disabled_hammer.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    disabled_shotgun.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    disabled_grenade.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    disabled_laser.extend(img_part);
                    let (img_part, _) = rest.split_at(segment_full_width * 2);
                    disabled_gun.extend(img_part);
                } else if y < segment_height * 3 * 2 {
                    let rest = y_chunk;

                    let (_, rest) = rest.split_at(segment_full_width * 2 * 2);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    tele_grenade.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    tele_pistol.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    tele_laser.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    deep_frozen.extend(img_part);
                    let (img_part, _) = rest.split_at(segment_full_width * 2);
                    live_frozen.extend(img_part);
                } else if y < segment_height * 4 * 2 {
                    let rest = y_chunk;

                    let (_, rest) = rest.split_at(segment_full_width * 2 * 2);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    disabled_finish.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    dummy_hammer.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    dummy_copy.extend(img_part);
                    let (img_part, rest) = rest.split_at(segment_full_width * 2);
                    stage_locked.extend(img_part);
                    let (img_part, _) = rest.split_at(segment_full_width * 2);
                    team0_mode.extend(img_part);
                }
            });
        Ok(DdraceHudConvertResult {
            jump: DdraceHudPart::new(jump, segment_width * 2, segment_height * 2),
            jump_used: DdraceHudPart::new(jump_used, segment_width * 2, segment_height * 2),
            solo: DdraceHudPart::new(solo, segment_width * 2, segment_height * 2),
            collision_off: DdraceHudPart::new(collision_off, segment_width * 2, segment_height * 2),
            endless_jump: DdraceHudPart::new(endless_jump, segment_width * 2, segment_height * 2),
            endless_hook: DdraceHudPart::new(endless_hook, segment_width * 2, segment_height * 2),
            jetpack: DdraceHudPart::new(jetpack, segment_width * 2, segment_height * 2),

            freeze_left: DdraceHudPart::new(freeze_left, segment_width * 2, segment_height),
            freeze_right: DdraceHudPart::new(freeze_right, segment_width * 2, segment_height),
            ninja_left: DdraceHudPart::new(ninja_left, segment_width * 2, segment_height),
            ninja_right: DdraceHudPart::new(ninja_right, segment_width * 2, segment_height),
            disabled_hook_others: DdraceHudPart::new(
                disabled_hook_others,
                segment_width * 2,
                segment_height * 2,
            ),
            disabled_hammer: DdraceHudPart::new(
                disabled_hammer,
                segment_width * 2,
                segment_height * 2,
            ),
            disabled_shotgun: DdraceHudPart::new(
                disabled_shotgun,
                segment_width * 2,
                segment_height * 2,
            ),
            disabled_grenade: DdraceHudPart::new(
                disabled_grenade,
                segment_width * 2,
                segment_height * 2,
            ),
            disabled_laser: DdraceHudPart::new(
                disabled_laser,
                segment_width * 2,
                segment_height * 2,
            ),
            disabled_gun: DdraceHudPart::new(disabled_gun, segment_width * 2, segment_height * 2),

            tele_grenade: DdraceHudPart::new(tele_grenade, segment_width * 2, segment_height * 2),
            tele_pistol: DdraceHudPart::new(tele_pistol, segment_width * 2, segment_height * 2),
            tele_laser: DdraceHudPart::new(tele_laser, segment_width * 2, segment_height * 2),
            deep_frozen: DdraceHudPart::new(deep_frozen, segment_width * 2, segment_height * 2),
            live_frozen: DdraceHudPart::new(live_frozen, segment_width * 2, segment_height * 2),

            disabled_finish: DdraceHudPart::new(
                disabled_finish,
                segment_width * 2,
                segment_height * 2,
            ),
            dummy_hammer: DdraceHudPart::new(dummy_hammer, segment_width * 2, segment_height * 2),
            dummy_copy: DdraceHudPart::new(dummy_copy, segment_width * 2, segment_height * 2),
            stage_locked: DdraceHudPart::new(stage_locked, segment_width * 2, segment_height * 2),
            team0_mode: DdraceHudPart::new(team0_mode, segment_width * 2, segment_height * 2),
        })
    }
}
