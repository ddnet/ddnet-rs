use std::{ops::ControlFlow, str::FromStr};

use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{parse_macro_input, Expr, Fields, FieldsNamed, ImplItem, Item, Meta, Stmt};

pub fn wasm_mod_prepare(
    _attr: TokenStream,
    tokens: TokenStream,
    interface_name: &str,
) -> TokenStream {
    let mut base_input = parse_macro_input!(tokens as Item);

    // go through all impls
    if let Item::Mod(mod_impl) = &mut base_input {
        // first find the trait GameStateInterface
        // this makes it ez to know all functions by name
        let mut func_names: Vec<String> = Default::default();
        let _ = mod_impl
            .content
            .as_ref()
            .unwrap()
            .1
            .iter()
            .try_for_each(|item| {
                if let Item::Impl(impl_impl) = item {
                    if let Some((_, trait_impl, _)) = &impl_impl.trait_ {
                        if trait_impl
                            .get_ident()
                            .is_some_and(|ident| ident == interface_name)
                        {
                            impl_impl.items.iter().for_each(|func| {
                                if let ImplItem::Fn(func) = func {
                                    if func.attrs.iter().any(|attr| {
                                        if let Meta::Path(path) = &attr.meta {
                                            path.segments.first().is_some()
                                                && path
                                                    .segments
                                                    .first()
                                                    .unwrap()
                                                    .ident
                                                    .to_string()
                                                    .contains("wasm_func_auto_call")
                                        } else {
                                            false
                                        }
                                    }) {
                                        func_names.push(func.sig.ident.to_string());
                                    }
                                }
                            });
                            return ControlFlow::Break(());
                        }
                    }
                }
                ControlFlow::Continue(())
            });

        // now find the struct name
        let _ = mod_impl
            .content
            .as_mut()
            .unwrap()
            .1
            .iter_mut()
            .try_for_each(|item| {
                if let Item::Struct(struct_impl) = item {
                    if let Fields::Named(named_fields) = &mut struct_impl.fields {
                        let mut build_func = "{".to_string();
                        func_names.iter().for_each(|name| {
                            build_func += &(name.clone() + "_name: wasmer::TypedFunction<(), ()>,");
                        });
                        build_func += "}";
                        let tokens = proc_macro2::TokenStream::from_str(&build_func).unwrap();
                        let joinable_struct = syn::parse::<FieldsNamed>(tokens.into()).unwrap();
                        named_fields.named.extend(joinable_struct.named);
                        return ControlFlow::Break(());
                    }
                }
                ControlFlow::Continue(())
            });

        // find the constructor impl and rewrite it
        let _ = mod_impl
            .content
            .as_mut()
            .unwrap()
            .1
            .iter_mut()
            .try_for_each(|item| {
                if let Item::Impl(impl_impl) = item {
                    if impl_impl.attrs.iter().any(|attr| {
                        if let Meta::Path(path) = &attr.meta {
                            path.segments.first().is_some()
                                && path
                                    .segments
                                    .first()
                                    .unwrap()
                                    .ident
                                    .to_string()
                                    .contains("constructor")
                        } else {
                            false
                        }
                    }) {
                        impl_impl.attrs.clear();

                        // find the new func
                        let _ = impl_impl.items.iter_mut().try_for_each(|func| {
                            if let ImplItem::Fn(func) = func {
                                if func.sig.ident == "new" {
                                    let mut res_token = func
                                        .block
                                        .stmts
                                        .pop()
                                        .unwrap()
                                        .to_token_stream()
                                        .to_string();
                                    let has_paren = func
                                        .sig
                                        .output
                                        .to_token_stream()
                                        .to_string()
                                        .replace(" ", "")
                                        .contains("Result<");
                                    res_token = res_token.chars().rev().collect::<String>();
                                    if has_paren {
                                        res_token = res_token.replacen(")", "", 1);
                                    }
                                    res_token = res_token.replacen("}", "", 1);
                                    res_token = res_token.replacen(",", "", 1);
                                    res_token = res_token.chars().rev().collect::<String>();

                                    let mut res_expr = res_token + ", ";
                                    func_names.iter().for_each(|name| {
                                        let var_name = name.clone() + "_name";
                                        res_expr += &(var_name.clone() + ", ");
                                        let mut func_init_stmt = "let ".to_string();
                                        func_init_stmt += &var_name;
                                        func_init_stmt += &(" = wasm_manager.run_func_by_name(\""
                                            .to_string()
                                            + name
                                            + "\");");
                                        func.block.stmts.push(
                                            syn::parse::<Stmt>(
                                                TokenStream::from_str(&func_init_stmt).unwrap(),
                                            )
                                            .unwrap(),
                                        );
                                    });
                                    res_expr += "}";
                                    if has_paren {
                                        res_expr += ")";
                                    }
                                    func.block.stmts.push(Stmt::Expr(
                                        syn::parse::<Expr>(
                                            TokenStream::from_str(&res_expr).unwrap(),
                                        )
                                        .unwrap(),
                                        None,
                                    ));
                                    return ControlFlow::Break(());
                                }
                            }
                            ControlFlow::Continue(())
                        });

                        return ControlFlow::Break(());
                    }
                }
                ControlFlow::Continue(())
            });
    }

    //panic!("{:?}", base_input.to_token_stream().to_string());
    base_input.to_token_stream().into()
}
