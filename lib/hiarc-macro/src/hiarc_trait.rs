use std::{num::NonZeroU64, str::FromStr};

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{GenericParam, Item, parse_macro_input, parse_quote};

pub(crate) fn hi_arc_trait_impl(
    tokens: TokenStream,
    micro: bool,
    forced_hi_val: Option<NonZeroU64>,
) -> TokenStream {
    let base_input = parse_macro_input!(tokens as Item);

    let output;

    let (name, generics, field_values) = match &base_input {
        Item::Enum(e) => {
            let name = &e.ident;
            let generics = &e.generics;
            let field_values: Vec<proc_macro2::TokenStream> = e
                .variants
                .iter()
                .flat_map(|a| {
                    if a.fields.is_empty() {
                        vec![proc_macro2::TokenStream::from_str("isize").unwrap()]
                    } else {
                        a.fields
                            .iter()
                            .filter(|f| {
                                !f.attrs.iter().any(|attr| {
                                    attr.meta
                                        .to_token_stream()
                                        .to_string()
                                        .to_lowercase()
                                        .contains("hiarc_skip_unsafe")
                                })
                            })
                            .map(|f| f.ty.clone().into_token_stream())
                            .collect()
                    }
                })
                .collect();
            (name.clone(), generics.clone(), field_values)
        }
        Item::Struct(s) => {
            let name = &s.ident;
            let generics = &s.generics;

            let field_values: Vec<proc_macro2::TokenStream> = s
                .fields
                .iter()
                .filter_map(|a| {
                    let ty = &a.ty;
                    if a.attrs.iter().any(|attr| {
                        attr.meta
                            .to_token_stream()
                            .to_string()
                            .to_lowercase()
                            .contains("hiarc_skip_unsafe")
                    }) {
                        None
                    } else {
                        Some(ty.into_token_stream())
                    }
                })
                .collect();
            (name.clone(), generics.clone(), field_values)
        }
        _ => {
            panic!(
                "this macro is only useful for enums and structs, found: {:?}",
                base_input.to_token_stream()
            )
        }
    };

    let mut typed_generics = generics.clone();
    typed_generics.params.iter_mut().for_each(|param| {
        match param {
            GenericParam::Lifetime(_) => {
                // ignore
            }
            GenericParam::Type(ty) => {
                ty.bounds.push(syn::TypeParamBound::Trait(parse_quote! {
                    hiarc::HiarcTrait
                }));
            }
            GenericParam::Const(_) => {
                // ignore
            }
        }
    });
    let generics_params: Vec<proc_macro2::TokenStream> = generics
        .params
        .into_iter()
        .map(|param| match param {
            GenericParam::Lifetime(ty) => ty.to_token_stream(),
            GenericParam::Type(ty) => ty.ident.to_token_stream(),
            GenericParam::Const(ty) => ty.ident.to_token_stream(),
        })
        .collect();
    let generics = quote! {
        <#(#generics_params),*>
    };

    if field_values.is_empty() {
        let val = forced_hi_val.unwrap_or(NonZeroU64::new(1).unwrap()).get();

        output = quote! {
            unsafe impl #typed_generics hiarc::HiarcTrait for #name #generics {
                const HI_VAL: u64 = #val;
            }
        };
        //panic!("{:?}", output.to_token_stream());
    } else {
        let field_len = field_values.len();
        let mut values: Vec<String> = field_values
            .into_iter()
            .map(|val| {
                "max(".to_string()
                    + &quote! {
                        <#val as hiarc::HiarcTrait>::HI_VAL
                    }
                    .to_string()
                    + ", "
            })
            .collect();
        values.push("0)".into());
        for _ in 0..field_len - 1 {
            values.push(")".into());
        }
        let values = proc_macro2::TokenStream::from_str(&values.join("")).unwrap();

        let forced_val = forced_hi_val
            .unwrap_or(NonZeroU64::new(u64::MAX).unwrap())
            .get();

        let val_token = if micro {
            quote!(val + 1)
        } else {
            quote!(val + 1000)
        };

        output = quote! {
            unsafe impl #typed_generics hiarc::HiarcTrait for #name #generics {
                const HI_VAL: u64 = {
                    const fn max(a: u64, b: u64) -> u64 {
                        [a, b][(a < b) as usize]
                    }
                    let val = #values;
                    assert!(#forced_val > val, "hierarchical value of this struct was smaller than the value of one of its attributes, which means that the strict hierarchy is violated");
                    if #forced_val != u64::MAX {
                        #forced_val
                    }
                    else {
                        #val_token
                    }
                };
            }
        };
    }

    //panic!("{:?}", output.to_token_stream().to_string());
    output.to_token_stream().into()
}
