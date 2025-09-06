mod guest_func_auto_impl;
mod guest_funcs;
mod mod_prepare;

use std::str::FromStr;

use guest_func_auto_impl::guest_func_call_from_host_auto_impl;
use guest_funcs::impl_guest_functions;
use mod_prepare::wasm_mod_prepare;
use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{Expr, FnArg, Item, Pat, ReturnType, Stmt, Type, parse_macro_input, token::Semi};

/// prepare a host function to automatically call a wasm function
#[proc_macro_attribute]
pub fn wasm_func_auto_call(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    let mut base_input = parse_macro_input!(tokens as Item);
    let no_res = attr.to_string() == "no_res";

    // go through all impls
    if let Item::Fn(fn_impl) = &mut base_input {
        fn_impl.block.stmts = Vec::new();
        // add args
        let mut arg_index = 0;
        for arg in &fn_impl.sig.inputs {
            if let FnArg::Typed(typed_arg) = arg {
                if let Pat::Ident(arg) = typed_arg.pat.as_ref() {
                    let mut arg_expr = arg.ident.to_string();
                    if let Type::Reference(_) = typed_arg.ty.as_ref() {
                        arg_expr += "";
                    } else {
                        arg_expr = format!("&{arg_expr}");
                    }
                    let func_call = syn::parse::<Expr>(
                        TokenStream::from_str(
                            &("self.wasm_manager.add_param( ".to_string()
                                + &arg_index.to_string()
                                + ", "
                                + &arg_expr
                                + " )"),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                    fn_impl
                        .block
                        .stmts
                        .push(Stmt::Expr(func_call, Some(Semi::default())));
                }
                arg_index += 1;
            }
        }
        let func_call = syn::parse::<Expr>(
            TokenStream::from_str(
                &("self.wasm_manager.run_by_ref(&self.".to_string()
                    + &(fn_impl.sig.ident.to_string() + "_name")
                    + ").unwrap()"),
            )
            .unwrap(),
        )
        .unwrap();
        fn_impl
            .block
            .stmts
            .push(Stmt::Expr(func_call, Some(Semi::default())));

        // if there is a result, parse it
        if !no_res {
            if let ReturnType::Type(_, _) = fn_impl.sig.output {
                let result = syn::parse::<Expr>(
                    TokenStream::from_str("self.wasm_manager.get_result_as::<_>()").unwrap(),
                )
                .unwrap();
                fn_impl.block.stmts.push(Stmt::Expr(result, None));
            }
        } else {
            fn_impl.sig.output = ReturnType::Default;
        }
    }

    //panic!("{:?}", base_input.to_token_stream().to_string());
    base_input.to_token_stream().into()
}

/// automatically fill the state of the Self object with typed function exports from
/// the wasm module
#[proc_macro_attribute]
pub fn wasm_mod_prepare_state(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    wasm_mod_prepare(attr, tokens, "GameStateInterface")
}

/// automatically fill the render_game of the Self object with typed function exports from
/// the wasm module
#[proc_macro_attribute]
pub fn wasm_mod_prepare_render_game(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    wasm_mod_prepare(attr, tokens, "RenderGameInterface")
}

/// automatically fill the editor of the Self object with typed function exports from
/// the wasm module
#[proc_macro_attribute]
pub fn wasm_mod_prepare_editor(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    wasm_mod_prepare(attr, tokens, "EditorInterface")
}

/// automatically fill the auto_mapper of the Self object with typed function exports from
/// the wasm module
#[proc_macro_attribute]
pub fn wasm_mod_prepare_auto_mapper(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    wasm_mod_prepare(attr, tokens, "AutoMapperInterface")
}

/// prepare a wasm function to automatically call a host function
#[proc_macro_attribute]
pub fn guest_func_call_from_host_auto(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    guest_func_call_from_host_auto_impl(attr, tokens)
}

/// this is just to signal the other macro, that this function is used when implementing the guest functions
/// for host access
#[proc_macro_attribute]
pub fn guest_func_call_from_host_auto_dummy(
    _attr: TokenStream,
    tokens: TokenStream,
) -> TokenStream {
    tokens
}

/// implements guest functions for an impl block automatically
/// removes any trait association
#[proc_macro_attribute]
pub fn impl_guest_functions_state(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_guest_functions(attr, tokens, "API_STATE")
}

#[proc_macro_attribute]
pub fn impl_guest_functions_render_game(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_guest_functions(attr, tokens, "API_RENDER_GAME")
}

#[proc_macro_attribute]
pub fn impl_guest_functions_editor(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_guest_functions(attr, tokens, "API_EDITOR")
}

#[proc_macro_attribute]
pub fn impl_guest_functions_auto_mapper(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    impl_guest_functions(attr, tokens, "API_AUTO_MAPPER")
}
