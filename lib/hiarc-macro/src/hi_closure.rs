use std::marker::PhantomData;

use proc_macro::TokenStream;
use proc_macro2::{Delimiter, TokenTree};
use quote::{ToTokens, quote};
use syn::{
    FnArg, GenericArgument, PatType, Result, Token, Type, braced, bracketed,
    parse::{Parse, ParseStream, Parser},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
    token::Bracket,
};

#[derive(Debug)]
struct CommaSeparatedTokenStream<P: Parse> {
    pub tokens: Vec<proc_macro2::TokenStream>,
    _p: PhantomData<P>,
}

impl<P: Parse> Parse for CommaSeparatedTokenStream<P> {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(CommaSeparatedTokenStream {
            tokens: {
                let mut res: Vec<proc_macro2::TokenStream> = Default::default();
                let mut next_tokens = proc_macro2::TokenStream::default();
                while !input.is_empty() {
                    let was_comma = input.step(|cursor| {
                        let mut rest = *cursor;
                        while let Some((tt, next)) = rest.token_tree() {
                            match &tt {
                                TokenTree::Punct(punct) if punct.as_char() == ',' => {
                                    return Ok((true, next));
                                }
                                _ => {
                                    next_tokens.extend(std::iter::once(tt));
                                    rest = next
                                }
                            }
                        }
                        Ok((false, rest))
                    })?;

                    // try to parse these tokens as `P`, else continue
                    if syn::parse::Parser::parse2(P::parse, next_tokens.clone()).is_ok() {
                        res.push(next_tokens);
                        next_tokens = Default::default();
                    } else if was_comma {
                        next_tokens.extend(quote!(,));
                    }
                }
                if !next_tokens.is_empty() {
                    res.push(next_tokens);
                }
                res
            },
            _p: Default::default(),
        })
    }
}

#[derive(Debug)]
struct HiClosureImpl {
    pub _or1_token: Token![|],
    pub inputs: CommaSeparatedTokenStream<PatType>,
    pub _or2_token: Token![|],
    pub output: proc_macro2::TokenStream,
    pub body: proc_macro2::TokenStream,
}

impl Parse for HiClosureImpl {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(HiClosureImpl {
            _or1_token: input.parse()?,
            inputs: {
                let mut next_tokens = proc_macro2::TokenStream::default();
                input.step(|cursor| {
                    let mut rest = *cursor;
                    while let Some((tt, next)) = rest.token_tree() {
                        match &tt {
                            TokenTree::Punct(punct) if punct.as_char() == '|' => {
                                return Ok(((), rest));
                            }
                            _ => {
                                next_tokens.extend(std::iter::once(tt));
                                rest = next
                            }
                        }
                    }
                    Ok(((), rest))
                })?;
                syn::parse::<CommaSeparatedTokenStream<PatType>>(next_tokens.into())?
            },
            _or2_token: input.parse()?,
            output: {
                let mut next_tokens = proc_macro2::TokenStream::default();
                input.step(|cursor| {
                    let mut rest = *cursor;
                    while let Some((tt, next)) = rest.token_tree() {
                        match &tt {
                            TokenTree::Group(grp) if Delimiter::Brace == grp.delimiter() => {
                                return Ok(((), rest));
                            }
                            _ => {
                                next_tokens.extend(std::iter::once(tt));
                                rest = next
                            }
                        }
                    }
                    Ok(((), rest))
                })?;
                next_tokens
            },
            body: {
                let content;
                braced!(content in input);
                content.parse()?
            },
        })
    }
}

#[derive(Debug)]
struct HiClosureSyn {
    generics: Option<(
        Token![<],
        Punctuated<GenericArgument, Token![,]>,
        Token![>],
        Token![,],
    )>,
    _captures_bracket: Bracket,
    captures: CommaSeparatedTokenStream<FnArg>,
    _comma: Token![,],
    closure: HiClosureImpl,
}

impl Parse for HiClosureSyn {
    fn parse(input: ParseStream) -> Result<Self> {
        let generics = if let Some(generic_left) = input.parse::<Option<Token![<]>>()? {
            let mut next_tokens = proc_macro2::TokenStream::default();
            input.step(|cursor| {
                let mut rest = *cursor;
                let mut opened_le = 0;
                while let Some((tt, next)) = rest.token_tree() {
                    match &tt {
                        TokenTree::Punct(punct) if punct.as_char() == '>' && opened_le == 0 => {
                            return Ok(((), next));
                        }
                        token => {
                            if let TokenTree::Punct(punct) = token {
                                if punct.as_char() == '>' {
                                    opened_le -= 1;
                                } else if punct.as_char() == '<' {
                                    opened_le += 1;
                                }
                            }
                            next_tokens.extend(std::iter::once(tt));
                            rest = next
                        }
                    }
                }
                Ok(((), rest))
            })?;
            let generics: Punctuated<GenericArgument, Token![,]> =
                Parser::parse2(Punctuated::parse_terminated, next_tokens)?;
            let generic_right: Token![>] = Default::default();
            let comma: Token![,] = input.parse()?;

            Some((generic_left, generics, generic_right, comma))
        } else {
            None
        };
        let content;
        let captures_bracket: Bracket = bracketed!(content in input);
        let comma = input.parse()?;
        Ok(HiClosureSyn {
            generics,
            _captures_bracket: captures_bracket,
            captures: content.parse()?,
            _comma: comma,
            closure: input.parse()?,
        })
    }
}

pub(crate) fn hi_closure_impl(item: TokenStream) -> TokenStream {
    let HiClosureSyn {
        generics,
        captures,
        closure,
        ..
    } = parse_macro_input!(item as HiClosureSyn);

    // check args
    let mut lifetimes = false;
    let mut moved_params = false;
    let mut mut_ref = false;
    type ArgRes = (
        Vec<proc_macro2::TokenStream>,
        (
            Vec<proc_macro2::TokenStream>,
            (Vec<proc_macro2::TokenStream>, Vec<proc_macro2::TokenStream>),
        ),
    );
    let (types, (call_assigns, (call_assigns_ref, struct_assigns))): ArgRes = captures.tokens
        .iter()
        .map(|arg_tokens| {
            let call_assigns;
            let call_assigns_ref;
            let struct_assigns;
            let arg = syn::parse::<FnArg>(arg_tokens.clone().into());
            match arg {
                Ok(mut arg) => {
                    match &mut arg {
                        FnArg::Receiver(_) => {
                            call_assigns = quote!();
                            call_assigns_ref = quote!();
                            struct_assigns = quote!{
                                compile_error!("self bindings are currently not supported, use `let selfi = self;` instead and bind `selfi`.")
                            };
                            // nothing to do
                        }
                        FnArg::Typed(ty) => {
                            let mut mut_ref_internal = false;
                            let mut is_ref_internal = false;
                            if let Type::Reference(r) = ty.ty.as_mut() {
                                lifetimes = true;
                                is_ref_internal = true;
                                r.lifetime = Some(parse_quote!('a));
                                if r.mutability.is_some() {
                                    mut_ref = true;
                                    mut_ref_internal = true;
                                }
                            } else {
                                moved_params = true;
                            }
                            let name = ty.pat.to_token_stream();
                            call_assigns = quote! {
                                let #name = self.#name;
                            };
                            call_assigns_ref = if is_ref_internal {
                                if mut_ref_internal {
                                    quote! {
                                        let #name = &mut *self.#name;
                                    }
                                } else {
                                    quote! {
                                        let #name = &*self.#name;
                                    }
                                }
                            }
                            else {
                                call_assigns.clone()
                            };
                            if !ty.attrs.is_empty() {
                                struct_assigns = quote!(compile_error!("attributes are not allowed"));
                            }
                            else {
                                struct_assigns = quote!(#name);
                            }
                        }
                    }
                    (arg.to_token_stream(), (call_assigns, (call_assigns_ref, struct_assigns)))
                },
                Err(err) => {
                    let err = err.to_string();
                    let err = quote!(compile_error!(#err));
                    (err.clone(), (err.clone(), (err.clone(), err.clone())))
                },
            }
        })
        .collect::<Vec<(
            proc_macro2::TokenStream,
            (proc_macro2::TokenStream,
            (proc_macro2::TokenStream, proc_macro2::TokenStream)),
        )>>()
        .into_iter()
        .unzip();

    let (closure_param_names, closure_param_types): (
        Vec<proc_macro2::TokenStream>,
        Vec<proc_macro2::TokenStream>,
    ) = closure
        .inputs
        .tokens
        .iter()
        .map(|input| {
            let pat = syn::parse::<PatType>(input.clone().into());
            match pat {
                Ok(ty) => {
                    // nothing to do
                    (ty.pat.to_token_stream(), ty.ty.to_token_stream())
                }
                Err(err) => {
                    let err = err.to_string();
                    let err = format!(
                        "currently only typed arguments are allowed: {err}, \
                        found: {input}",
                    );
                    (
                        quote!(),
                        quote!(compile_error!(
                            #err
                        )),
                    )
                }
            }
        })
        .collect::<Vec<(proc_macro2::TokenStream, proc_macro2::TokenStream)>>()
        .into_iter()
        .unzip();

    let res_ty = {
        let output = syn::parse::<syn::ReturnType>(closure.output.clone().into());
        match output {
            Ok(output) => {
                if let syn::ReturnType::Type(_, ty) = &output {
                    ty.to_token_stream()
                } else {
                    let err = output.to_token_stream().to_string();
                    let err = format!("result type must be explicitly set: {err}");
                    quote!(compile_error!(#err))
                }
            }
            Err(err) => {
                let err = format!(
                    "result type must be explicitly set: {err} from {}",
                    closure.output.clone()
                );
                quote!(compile_error!(#err))
            }
        }
    };

    let (generics, generic_names, generics_where) = if let Some((_, generics, _, _)) = generics {
        let generic_names: Vec<_> = generics
            .iter()
            .map(|g| {
                if let GenericArgument::Constraint(generic) = g {
                    generic.ident.to_token_stream()
                } else {
                    g.to_token_stream()
                }
            })
            .collect();
        let generics: Vec<_> = generics.into_iter().collect();
        if lifetimes {
            (
                quote!(<'a, #(#generics),*>),
                quote!(<'a, #(#generic_names),*>),
                quote!(where #(#generic_names: hiarc::HiarcTrait),*),
            )
        } else {
            (
                quote!(<#(#generics),*>),
                quote!(<#(#generic_names),*>),
                quote!(where #(#generic_names: hiarc::HiarcTrait),*),
            )
        }
    } else if lifetimes {
        (quote!(<'a>), quote!(<'a>), quote!())
    } else {
        (quote!(), quote!(), quote!())
    };

    let closure_block = &closure.body;

    let fn_mut_impl = if lifetimes || captures.tokens.is_empty() {
        quote! {
            unsafe impl #generics hiarc::HiFnMutBase<(#(#closure_param_types),*), #res_ty> for InternalHiClosureImpl #generic_names #generics_where {
                fn call_mut(&mut self, (#(#closure_param_names),*): (#(#closure_param_types),*)) -> #res_ty {
                    #(#call_assigns_ref)*
                    #closure_block
                }
            }

            unsafe impl #generics hiarc::HiFnMut<(#(#closure_param_types),*), #res_ty> for InternalHiClosureImpl #generic_names #generics_where {}
        }
    } else {
        quote!()
    };

    let fn_impl = if (lifetimes && !mut_ref) || captures.tokens.is_empty() {
        quote! {
            unsafe impl #generics hiarc::HiFnBase<(#(#closure_param_types),*), #res_ty> for InternalHiClosureImpl #generic_names #generics_where {
                fn call_ref(&self, (#(#closure_param_names),*): (#(#closure_param_types),*)) -> #res_ty {
                    #(#call_assigns_ref)*
                    #closure_block
                }
            }

            unsafe impl #generics hiarc::HiFn<(#(#closure_param_types),*), #res_ty> for InternalHiClosureImpl #generic_names #generics_where {}
        }
    } else {
        quote!()
    };

    quote! {
        {
            #[derive(hiarc::Himicro)]
            pub struct InternalHiClosureImpl #generics {
                #(#types),*
            }

            unsafe impl #generics hiarc::HiFnOnceBase<(#(#closure_param_types),*), #res_ty> for InternalHiClosureImpl #generic_names #generics_where {
                fn call_once(mut self, (#(#closure_param_names),*): (#(#closure_param_types),*)) -> #res_ty {
                    #(#call_assigns)*
                    #closure_block
                }
            }

            unsafe impl #generics hiarc::HiFnOnce<(#(#closure_param_types),*), #res_ty> for InternalHiClosureImpl #generic_names #generics_where {}

            #fn_mut_impl

            #fn_impl

            InternalHiClosureImpl {
                #(#struct_assigns),*
            }
        }
    }.into()
}
