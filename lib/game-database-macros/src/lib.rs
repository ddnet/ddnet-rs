use proc_macro::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Item, parse_macro_input};

#[proc_macro_derive(StatementArgs)]
pub fn statement_args(tokens: TokenStream) -> TokenStream {
    let base_input = parse_macro_input!(tokens as Item);

    match &base_input {
        Item::Struct(s) => {
            let index_fields = s
                .fields
                .iter()
                .map(|field| {
                    let ident = &field.ident;

                    quote! {
                        #ident
                    }
                })
                .collect::<Vec<_>>();

            let field_db_fields = index_fields
                .iter()
                .map(|i| quote!(game_database::types::DbType::from(self.#i.clone())))
                .collect::<Vec<_>>();

            let field_indices = index_fields
                .iter()
                .enumerate()
                .map(|(i, _)| i)
                .collect::<Vec<_>>();

            let mut res = proc_macro2::TokenStream::default();

            let ident = &s.ident;
            let index_ident = format_ident!("{}Index", ident);

            res.extend(quote! {
                impl game_database::traits::DbStatementArgInterface for #ident {
                    fn to_db_args(&self) -> Vec<game_database::types::DbType> {
                        let mut res: Vec<game_database::types::DbType> = Default::default();

                        #(
                            res.push(#field_db_fields);
                        )*

                        res
                    }
                }

                impl game_database::traits::DbStatementArgIndexInterface<#index_ident> for #ident {
                    fn arg_indices() -> #index_ident { Default::default() }
                }
            });

            res.extend(quote! {
                pub struct #index_ident {
                    #(#index_fields: usize,)*
                }

                impl Default for #index_ident {
                    fn default() -> Self {
                        Self {
                            #(#index_fields: #field_indices,)*
                        }
                    }
                }
            });

            res.into()
        }
        _ => {
            let mut res = proc_macro2::TokenStream::default();

            res.extend(
                quote! { compile_error!("this derive macro is only inteded to be used on structs")},
            );

            res.into()
        }
    }
}

#[proc_macro_derive(StatementResult)]
pub fn statement_res(tokens: TokenStream) -> TokenStream {
    let base_input = parse_macro_input!(tokens as Item);

    match &base_input {
        Item::Struct(s) => {
            let index_fields_and_types = s
                .fields
                .iter()
                .map(|field| {
                    let ident = &field.ident;
                    (
                        (
                            ident.as_ref().map(|s| s.to_string()).unwrap(),
                            quote! {
                                #ident
                            },
                        ),
                        field.ty.to_token_stream(),
                    )
                })
                .collect::<Vec<_>>();

            let field_db_fields = index_fields_and_types
                .iter()
                .map(|(_, f)| quote!(game_database::types::DbType::from(<#f>::default())))
                .collect::<Vec<_>>();

            let (names_and_indices, _): (Vec<_>, Vec<_>) =
                index_fields_and_types.into_iter().unzip();
            let (field_names_str, index_fields): (Vec<_>, Vec<_>) =
                names_and_indices.into_iter().unzip();

            let mut res = proc_macro2::TokenStream::default();

            let ident = &s.ident;

            res.extend(quote! {
                impl game_database::traits::DbStatementResultInterface for #ident {
                    fn new(mut results: std::collections::HashMap<String, game_database::types::DbType>) -> anyhow::Result<Self>
                    where
                        Self: Sized
                    {
                        Ok(Self {
                            #(
                                #index_fields : results.remove(#field_names_str).ok_or_else(|| anyhow::anyhow!("no field with name \"{}\" was found.", #field_names_str))?.try_into()? ,
                            )*
                        })
                    }

                    fn mapping() -> std::collections::HashMap<String, game_database::types::DbType> {
                        let mut res: std::collections::HashMap<_, _> = Default::default();

                        #(
                            res.insert(#field_names_str .to_string(), #field_db_fields);
                        )*

                        res
                    }
                }
            });

            res.into()
        }
        _ => {
            let mut res = proc_macro2::TokenStream::default();

            res.extend(
                quote! { compile_error!("this derive macro is only inteded to be used on structs")},
            );

            res.into()
        }
    }
}
