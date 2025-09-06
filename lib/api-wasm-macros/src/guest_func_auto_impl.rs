use std::str::FromStr;

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{Expr, FnArg, Item, Pat, ReturnType, Stmt, Type, parse_quote};

pub fn guest_func_call_from_host_auto_impl(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    let behind_option = attr.to_string() == "option";

    // go through all impls
    let base_input = move || {
        let mut base_input =
            syn::parse::<Item>(tokens.clone()).map_err(|err| (tokens.into(), err.to_string()))?;
        if let Item::Fn(fn_impl) = &mut base_input {
            let func_name = fn_impl.sig.ident.clone();
            fn_impl.block.stmts = Vec::new();
            // add args
            let mut arg_index: u32 = 0;
            let mut func_args: Vec<proc_macro2::TokenStream> = Default::default();
            for arg in &fn_impl.sig.inputs {
                if let FnArg::Typed(typed_arg) = arg {
                    if let Pat::Ident(arg) = typed_arg.pat.as_ref() {
                        let arg_expr = arg.ident.clone();
                        let arg_type = if let Type::Reference(ref_type) = typed_arg.ty.as_ref() {
                            if ref_type.mutability.is_some() {
                                func_args.push(quote!(&mut #arg_expr));
                                return Err((base_input.to_token_stream(),
                                    "mutably referenced argument types are currently not supported,\
                                    since WASM does not take any references from the host or vice versa.\
                                    please manually implement this call or use return types instead of passing mut ref.".to_string()
                                ));
                                // ref_type.elem.to_token_stream().to_string()
                            } else {
                                func_args.push(quote!(& #arg_expr));
                                ref_type.elem.to_token_stream()
                            }
                        } else {
                            func_args.push(quote!(#arg_expr));
                            typed_arg.ty.to_token_stream()
                        };
                        let arg_type_str = arg_type.to_string();
                        let func_name_str = func_name.to_string();
                        let func_param_assign: Stmt = parse_quote!(
                            let #arg_expr: #arg_type = read_param_from_host_ex(
                                #arg_index,
                                #arg_type_str,
                                #func_name_str
                            );
                        );
                        fn_impl.block.stmts.push(func_param_assign);
                    }
                    arg_index += 1;
                }
            }

            fn_impl.sig.inputs.clear();
            fn_impl
                .sig
                .inputs
                .push_value(FnArg::Receiver(parse_quote!(&self)));

            let option_expr = if behind_option {
                quote!(.borrow_mut().as_mut().unwrap())
            } else {
                quote!()
            };

            let func_call: Stmt =
                parse_quote!(let res = self.state #option_expr.#func_name (#(#func_args),*););
            fn_impl.block.stmts.push(func_call);

            // if there is a return type, upload the return type and set the return type of this function to nothing
            if let ReturnType::Type(_, res_type) = &fn_impl.sig.output {
                if let Type::Reference(_) = res_type.as_ref() {
                    return Err((base_input.to_token_stream(),
                                "referenced return types are currently not supported, since WASM does not take \
                                any references from the host or vice versa. please manually implement this call".to_string())
                            );
                }
                let result = syn::parse::<Expr>(
                    TokenStream::from_str("upload_return_val::<_>(res)").unwrap(),
                )
                .unwrap();
                fn_impl.block.stmts.push(Stmt::Expr(result, None));
            }
            fn_impl.sig.output = ReturnType::Default;
        }
        Ok(base_input.to_token_stream())
    };

    match base_input() {
        Ok(res) => res.into(),
        Err((mut res, err)) => {
            res.extend(quote!(compile_error!(#err)));
            res.into()
        }
    }
}
