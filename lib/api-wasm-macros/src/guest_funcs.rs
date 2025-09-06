use proc_macro::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{FnArg, ImplItem, Item, Pat, Stmt, StmtMacro, parse_macro_input, parse_quote};

pub fn impl_guest_functions(
    _attr: TokenStream,
    tokens: TokenStream,
    var_name: &str,
) -> TokenStream {
    let mut base_input = parse_macro_input!(tokens as Item);

    let mut original = base_input.clone();

    if let Item::Impl(fn_impl) = &mut base_input {
        fn_impl.trait_ = None;
    }

    let mut res = base_input.to_token_stream();

    // if a trait was used then
    // the original gets functions with todo!() statement
    // this is just, so the compiler says if a trait impl is missing
    if let Item::Impl(fn_impl) = &mut original
        && fn_impl.trait_.is_some()
    {
        fn_impl.items.iter_mut().for_each(|item| {
            if let ImplItem::Fn(func) = item {
                // clear func attributes
                func.attrs.clear();
                func.sig.inputs.iter_mut().for_each(|inp| {
                    if let FnArg::Typed(ty) = inp
                        && let Pat::Ident(ident) = ty.pat.as_mut()
                    {
                        ident.ident = format_ident!("_{}", ident.ident);
                    }
                });
                func.block.stmts = vec![Stmt::Macro(StmtMacro {
                    mac: parse_quote! {
                        todo!()
                    },
                    attrs: Default::default(),
                    semi_token: Default::default(),
                })];
            }
        });
        res.extend(original.to_token_stream());
    }

    // implement the public guest functions (the ones visible to the host)
    let mut guest_funcs: proc_macro2::TokenStream = Default::default();
    if let Item::Impl(fn_impl) = &mut base_input {
        for func in &fn_impl.items {
            if let ImplItem::Fn(func_impl) = func
                && let Some(is_dummy) = func_impl.attrs.iter().find_map(|attr| {
                    let meta_str = attr.meta.to_token_stream().to_string();
                    if meta_str.contains("guest_func_call_from_host_auto") {
                        Some(meta_str.contains("guest_func_call_from_host_auto_dummy"))
                    } else {
                        None
                    }
                })
            {
                let func_name = func_impl.sig.ident.clone();
                let func_stmts = if is_dummy {
                    Vec::new()
                } else {
                    func_impl.block.stmts.clone()
                };

                let var_ident = format_ident!("{}", var_name);
                guest_funcs.extend(quote!(
                    #[unsafe(no_mangle)]
                    pub fn #func_name () {
                        #var_ident.with(|g| g.#func_name());
                        #(#func_stmts)*
                    }
                ));
            }
        }
        if !fn_impl.items.is_empty() {
            res.extend(guest_funcs);
        }
    }

    //panic!("{:?}", res.to_token_stream().to_string());
    res.into()
}
