use std::str::FromStr;

use proc_macro::{TokenStream, TokenTree};
use quote::{format_ident, ToTokens};
use syn::{parse::Parse, parse_macro_input, ImplItem, Item, TraitItem};
/*
fn rewrite_exact_mod_name(tokens: &mut String, mod_name: &str, new_mod_name: &str) {
    let regex =
        Regex::new(&("(".to_string() + mod_name + ")([ \n\r\t]*::[ \n\r\t]*[A-Z]+)")).unwrap();
    *tokens = regex
        .replace_all(tokens, |cap: &regex::Captures<'_>| {
            new_mod_name.to_string() + &cap[2].to_string()
        })
        .to_string();
}

fn rewrite_use_crate(tokens: &mut String) {
    let regex = Regex::new("(crate)([ \n\r\t]*::[ \n\r\t]*[A-Za-z]+)").unwrap();
    *tokens = regex
        .replace_all(tokens, |cap: &regex::Captures<'_>| {
            "game_base".to_string() + &cap[2].to_string()
        })
        .to_string();
} */
struct ItemVec {
    items: Vec<Item>,
}

impl Parse for ItemVec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut res: Vec<Item> = Default::default();
        while let Ok(item) = input.parse() {
            res.push(item);
        }
        Ok(Self { items: res })
    }
}

impl ToTokens for ItemVec {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        for item in &self.items {
            tokens.extend(item.to_token_stream());
        }
    }
}

fn impl_mod(base_tokens: TokenStream, mod_tokens: TokenStream) -> TokenStream {
    let mut base_input = parse_macro_input!(base_tokens as ItemVec);
    let mod_input = parse_macro_input!(mod_tokens as Item);

    // go through all impls
    if let Item::Mod(mod_input) = mod_input {
        let base_input = base_input.items.last_mut().unwrap();
        if let Item::Mod(base_input) = base_input {
            assert!(base_input.ident == mod_input.ident, "mod name differed");

            // rewrite base uses
            /*for item in &mut base_input.content.as_mut().unwrap().1 {
                if let Item::Use(base_use) = item {
                    if let UseTree::Path(base_use_path) = &mut base_use.tree {
                        assert_ne!(
                            base_use_path.ident, "super",
                            "please rewrite the base use to use crate:: instead of super::"
                        );
                        if base_use_path.ident == "crate" {
                            base_use_path.ident = syn::Ident::new("game_base", Span::call_site());
                        }
                    } else {
                        let mut tokens = base_use.to_token_stream().to_string();
                        rewrite_use_crate(&mut tokens);
                        *base_use = syn::parse(TokenStream::from_str(&tokens).unwrap()).unwrap()
                    }
                }
            }*/

            'next_item: for item in mod_input.content.unwrap().1 {
                match item {
                    Item::Impl(mod_impl) => {
                        // find all items of this impl in the base impls
                        'next_impl_item: for impl_item in mod_impl.items {
                            match impl_item {
                                ImplItem::Const(_) => todo!(),
                                ImplItem::Fn(mut func) => {
                                    let func_name = func.sig.ident.to_string();
                                    // find the funcion in the base impls
                                    for item_base in &mut base_input.content.as_mut().unwrap().1 {
                                        if let Item::Impl(base_impl) = item_base {
                                            if base_impl.self_ty != mod_impl.self_ty {
                                                continue;
                                            }
                                            for base_impl_item in base_impl.items.iter_mut() {
                                                match base_impl_item {
                                                    ImplItem::Const(_) => todo!(),
                                                    ImplItem::Fn(base_func) => {
                                                        if base_func.sig.ident == func_name {
                                                            let mut needs_super = false;
                                                            func.attrs.retain(|attr| {
                                                                let has_needs_super = attr
                                                                    .meta
                                                                    .to_token_stream()
                                                                    .to_string()
                                                                    == "needs_super";
                                                                needs_super |= has_needs_super;
                                                                !has_needs_super
                                                            });
                                                            std::mem::swap(base_func, &mut func);

                                                            if needs_super
                                                                && base_impl.trait_.is_none()
                                                            {
                                                                func.sig.ident = format_ident!(
                                                                    "_super_{}",
                                                                    func_name
                                                                );
                                                                base_impl
                                                                    .items
                                                                    .push(ImplItem::Fn(func));
                                                            }
                                                            continue 'next_impl_item;
                                                        }
                                                    }
                                                    ImplItem::Type(_) => todo!(),
                                                    ImplItem::Macro(_) => todo!(),
                                                    ImplItem::Verbatim(_) => todo!(),
                                                    _ => todo!(),
                                                }
                                            }
                                        }
                                    }
                                }
                                ImplItem::Type(_) => todo!(),
                                ImplItem::Macro(_) => todo!(),
                                ImplItem::Verbatim(_) => todo!(),
                                _ => todo!(),
                            }
                        }
                    }
                    // add mod uses
                    Item::Use(mod_use) => {
                        base_input
                            .content
                            .as_mut()
                            .unwrap()
                            .1
                            .insert(0, Item::Use(mod_use));
                    }
                    Item::Const(_) => todo!(),
                    Item::Enum(mod_enum) => {
                        let content = &mut base_input.content.as_mut().unwrap().1;
                        for item_base in content.iter_mut() {
                            if let Item::Enum(base_enum) = item_base {
                                if mod_enum.ident == base_enum.ident {
                                    *base_enum = mod_enum;
                                    continue 'next_item;
                                }
                            }
                        }
                        content.push(Item::Enum(mod_enum));
                    }
                    Item::ExternCrate(_) => todo!(),
                    Item::Fn(_) => todo!(),
                    Item::ForeignMod(_) => todo!(),
                    Item::Macro(_) => todo!(),
                    Item::Mod(_) => todo!(),
                    Item::Static(_) => todo!(),
                    Item::Struct(mod_struct) => {
                        let content = &mut base_input.content.as_mut().unwrap().1;
                        for item_base in content.iter_mut() {
                            if let Item::Struct(base_struct) = item_base {
                                if mod_struct.ident == base_struct.ident {
                                    *base_struct = mod_struct;
                                    continue 'next_item;
                                }
                            }
                        }
                        content.push(Item::Struct(mod_struct));
                    }
                    Item::Trait(mod_trait) => {
                        let mod_trait_name = mod_trait.ident.to_string();
                        // find all items of this impl in the base impls
                        for impl_item in mod_trait.items {
                            match impl_item {
                                TraitItem::Const(_) => todo!(),
                                TraitItem::Fn(mut func) => {
                                    let func_name = func.sig.ident.to_string();
                                    // find the funcion in the base impls
                                    for item_base in &mut base_input.content.as_mut().unwrap().1 {
                                        if let Item::Trait(base_trait) = item_base {
                                            let base_trait_name = base_trait.ident.to_string();
                                            if mod_trait_name == base_trait_name {
                                                for base_trait_item in &mut base_trait.items {
                                                    match base_trait_item {
                                                        TraitItem::Const(_) => todo!(),
                                                        TraitItem::Fn(base_func) => {
                                                            if base_func.sig.ident == func_name {
                                                                std::mem::swap(
                                                                    base_func, &mut func,
                                                                );
                                                                break;
                                                            }
                                                        }
                                                        TraitItem::Type(_) => todo!(),
                                                        TraitItem::Macro(_) => todo!(),
                                                        TraitItem::Verbatim(_) => todo!(),
                                                        _ => todo!(),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                TraitItem::Type(_) => todo!(),
                                TraitItem::Macro(_) => todo!(),
                                TraitItem::Verbatim(_) => todo!(),
                                _ => todo!(),
                            }
                        }
                    }
                    Item::TraitAlias(_) => todo!(),
                    Item::Type(_) => todo!(),
                    Item::Union(_) => todo!(),
                    Item::Verbatim(_) => todo!(),
                    _ => todo!(),
                }
            }
        }
    }

    //panic!("{}", base_input.to_token_stream().to_string());
    base_input.items.last().unwrap().to_token_stream().into()
}

fn get_tokens_from_file(attr: TokenTree, file_path: &str) -> TokenStream {
    let mut path_to_ddnet_src = attr.to_string();
    path_to_ddnet_src = path_to_ddnet_src[1..path_to_ddnet_src.len() - 1].to_string();
    let to_mod_src_file =
        std::env::var("CARGO_MANIFEST_DIR").unwrap() + "/" + &path_to_ddnet_src + file_path;
    let file_src = std::fs::read_to_string(&to_mod_src_file).unwrap();
    TokenStream::from_str(&file_src).unwrap()
}

#[proc_macro_attribute]
pub fn entity_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/entities/entity.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn character_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    let mut iter = attr.into_iter();
    let path = iter.next().unwrap();
    // ignore comma
    iter.next();
    //let char_core_mod_name = iter.next().unwrap();
    impl_mod(
        get_tokens_from_file(path, "game/vanilla/src/entities/character.rs"),
        tokens,
        /* TODO: |tokens: &mut String| {
            rewrite_exact_mod_name(
                tokens,
                "character_core",
                char_core_mod_name.to_string().as_str(),
            );
        },*/
    )
}

#[proc_macro_attribute]
pub fn character_core_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/entities/character/core.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn projectile_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/entities/projectile.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn pickup_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/entities/pickup.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn flag_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/entities/flag.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn laser_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/entities/laser.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn weapon_def_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/weapons/definitions.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn collision_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/collision.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn player_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/entities/character/player.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn events_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/events.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn game_objects_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/game_objects.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn config_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/config.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn simulation_pipe_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/simulation_pipe.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn snapshot_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/snapshot.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn stage_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/stage.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn state_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/state.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn world_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/world.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn match_manager_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/match_manager.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn match_state_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/match_state.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn ctf_controller_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/ctf_controller.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn types_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "game/vanilla/src/types.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn graphics_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "lib/graphics/src/graphics.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn handle_backend_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "lib/graphics/src/handles/backend.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn handle_buffer_object_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "lib/graphics/src/handles/buffer_object.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn handle_shader_storage_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "lib/graphics/src/handles/shader_storage.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn handle_canvas_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "lib/graphics/src/handles/canvas.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn handle_quad_container_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "lib/graphics/src/handles/quad_container.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn handle_stream_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "lib/graphics/src/handles/stream.rs",
        ),
        tokens,
    )
}

#[proc_macro_attribute]
pub fn handle_texture_mod(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_mod(
        get_tokens_from_file(
            attr.into_iter().next().unwrap(),
            "lib/graphics/src/handles/texture.rs",
        ),
        tokens,
    )
}
