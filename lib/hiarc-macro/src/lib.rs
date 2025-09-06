mod hi_closure;
mod hiarc_trait;
mod safe_wrapper;

use std::{num::NonZeroU64, str::FromStr};

use hi_closure::hi_closure_impl;
use hiarc_trait::hi_arc_trait_impl;
use proc_macro::TokenStream;
use quote::ToTokens;
use safe_wrapper::{
    arc_mutex::hiarc_safer_arc_mutex_impl, rc_refcell::hiarc_safer_rc_refcell_impl,
    refcell::hiarc_safer_refcell_impl,
};
use syn::{Item, parse_macro_input};

/// the `Hiarc` derive macro can be used to proof hierarchy of other types that implement
/// this macro at compile time, which leads to a strict and clear hierarchy.
///
/// In other words if `struct A` exists and `struct B` has a field of type `A` => `A` can not
/// have a field of type `B`, because that would break the hierarchy
///
/// you can declare fields with the attribute `#[hiarc_skip_unsafe]` to tell this macro that it should
/// ignore this field
#[proc_macro_derive(Hiarc, attributes(hiarc_skip_unsafe))]
pub fn hi_arc_derive(tokens: TokenStream) -> TokenStream {
    hi_arc_trait_impl(tokens, false, None)
}

/// Like [`Hiarc`], but creates a smaller hiarc value offset.
///
/// This is rarely useful and mostly for internal use.
#[proc_macro_derive(Himicro, attributes(hiarc_skip_unsafe))]
pub fn hi_arc_derive_small(tokens: TokenStream) -> TokenStream {
    hi_arc_trait_impl(tokens, true, None)
}

/// this is similar to the `Hiarc` derive macro.
/// - For structs it allows to forcefully set the value of the hierarchy
///   `#[hiarc(10)]`. It's generally not recommended to do this, but can be great to
///   give the hierarchy an offset.
///   Consider for example that you have some backend that implements `Hiarc` and
///   some other struct `A` that implements `Hiarc`. If you want to make clear that both
///   are hierachically on the same level you could annotate both with `#[hiarc(1)]`.
///   They could not include theirselves.
///   A `#[hiarc(0)]` would be on the same level as normal std types, which would mean
///   that you can prevent using numeric types etc. in your struct, which is never useful.
///
/// # Examples
///
/// This will not compile:
/// ```no_run
/// use hiarc_macro::hiarc;
/// #[hiarc]
/// pub struct A {
///     b: Option<Box<B>>,
/// }
///
/// #[hiarc]
/// pub struct B {
///     b: Option<Box<A>>,
/// }
///
/// let _ = A { b: None };
/// ```
#[proc_macro_attribute]
pub fn hiarc(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    let forced_val = if attr.to_string() != "" {
        Some(
            NonZeroU64::from_str(&attr.to_string())
                .unwrap_or_else(|_| panic!("hi_arc attribute value must be a non-zero u64 value")),
        )
    } else {
        None
    };
    let trait_impl = hi_arc_trait_impl(tokens.clone(), false, forced_val);
    let mut base = parse_macro_input!(tokens as Item);

    match &mut base {
        Item::Struct(s) => {
            // remove attributes
            s.fields.iter_mut().for_each(|field| {
                field.attrs.retain(|attr| {
                    !attr
                        .meta
                        .to_token_stream()
                        .to_string()
                        .starts_with("hiarc_skip_unsafe")
                });
            });
        }
        Item::Enum(e) => e.variants.iter_mut().for_each(|v| {
            v.fields.iter_mut().for_each(|field| {
                field.attrs.retain(|attr| {
                    !attr
                        .meta
                        .to_token_stream()
                        .to_string()
                        .starts_with("hiarc_skip_unsafe")
                });
            });
        }),
        _ => {
            // nothing to do
        }
    }

    let mut tokens: TokenStream = base.to_token_stream().into();
    tokens.extend(trait_impl);
    tokens
}

/// This rewrites the public interface of a struct, wraps the original struct as if it would be in a
/// `Rc<RefCell<_>>>`, but with certain limitations and thus makes it safer to share.
///
/// Additionally it disallows any kind of borrowing of the original struct
/// without invoking `unsafe` code.
///
/// This attribute must be used on both:
/// - struct
/// - impl of the struct (even traits)
///
/// This attribute purposely does not re-implement the `Drop`-trait on the outer struct
///
/// You can additionally not pass any closures, closures can have bindings to an instance of the annotated struct and call
/// a function on it, which leads to panics.
/// Even fruther, all function parameters must implement hiarc and their hiarc value must be smaller than
/// the annotated struct => the parameter can not be a struct that keeps a reference to the annotated struct.
/// This is important, because the parameter object could e.g. implement the `Drop`-trait where it causes a call to
/// a function of the annotated struct.
///
/// The `hiarc_force_impl` can be used as an attribute on a function to tell the macro that
/// this function requires an implementation, even if it's a private function.
///
/// The `hiarc_trait_is_immutable_self` attribute can be used to implement a member of a trait as `&mut self`,
/// even if the trait itself should be immutable.
///
/// The `sync_send_wrapper` attribute can be used (`hiarc_macro::hiarc_safer_rc_refcell(sync_send_wrapper)`) can be used
/// to automatically get a wrapper that allows you to easily share your instance across other threads, if your instance is not shared.
/// see [`try_into_sync_send_wrapper`]. Note that your base type must be [`Sync`] + [`Send`] to make it compile.
/// The wrapper will be called <Name>SyncSend
///
/// # Examples
///
/// ```rust
/// #[hiarc_macro::hiarc_safer_rc_refcell]
/// #[derive(hiarc::Hiarc)]
/// pub struct MyStruct {}
///
/// #[hiarc_macro::hiarc_safer_rc_refcell]
/// impl MyStruct {
///     pub fn new() -> Self {
///         Self {}
///     }
///
///     pub fn test(&self, arg: i32) -> i32 {
///         arg
///     }
/// }
///
/// let s = MyStruct::new();
/// let t = s.clone();
/// assert!(s.test(3) == 3);
/// assert!(t.test(4) == 4);
/// ```
///
/// ```compile_fail
/// #[hiarc_macro::hiarc_safer_rc_refcell]
/// #[derive(Debug, hiarc_macro::Hiarc)]
/// pub struct MyBaseStruct {}
///
/// #[hiarc_macro::hiarc_safer_rc_refcell]
/// impl MyBaseStruct {
///     pub fn new() -> Self {
///         Self {}
///     }
///
///     // should already fail at compile time
///     pub fn this_wont_work(&self, arg: &MyStruct) {
///         println!("it does not work: {arg:?}");
///     }
/// }
///
/// #[derive(Debug, hiarc_macro::Hiarc)]
/// pub struct MyStruct {
///     // because MyStruct contains MyBaseStruct
///     // it will have a higher hierarchical value
///     // => MyBaseStruct, thanks to `hiarc_safer_rc_refcell`,
///     // will not accept it as argument for any function
///     base: MyBaseStruct,
/// }
///
/// impl MyStruct {
///     pub fn new() -> Self {
///         Self {
///             base: MyBaseStruct::new(),
///         }
///     }
/// }
///
/// use hiarc::HiarcTrait;
/// let s = MyStruct::new();
/// let b = s.base.clone();
/// b.this_wont_work(&s);
/// ```
#[proc_macro_attribute]
pub fn hiarc_safer_rc_refcell(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    hiarc_safer_rc_refcell_impl(attr, tokens)
}

/// similar to [`hiarc_safer_rc_refcell`] but without cloning (because no [`std::rc::Rc`])
#[proc_macro_attribute]
pub fn hiarc_safer_refcell(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    hiarc_safer_refcell_impl(attr, tokens)
}

/// similar to [`hiarc_safer_rc_refcell`], just thread safe
#[proc_macro_attribute]
pub fn hiarc_safer_arc_mutex(attr: TokenStream, tokens: TokenStream) -> TokenStream {
    hiarc_safer_arc_mutex_impl(attr, tokens)
}

/// Create a "hierarchical"-safe closure, which is interesting for
/// passing closures to a function which is modified by [`hiarc_safer_rc_refcell`].
///
/// It is a bit more annoying to type than normal closures, since captures must be
/// given explicitly and additionally even their type.
/// Optional generic arguments for the closure can be passed before the captures:
/// ```no_run
/// hi_closure!(<A>, [capt: MyType<A>], || -> () {})
/// ```
/// where A has to satisfy [`hiarc::HiarcTrait`].
///
/// # Examples
///
/// ```rust
/// #[hiarc_macro::hiarc_safer_rc_refcell]
/// #[derive(hiarc_macro::Hiarc)]
/// pub struct MyStruct {}
///
/// #[hiarc_macro::hiarc_safer_rc_refcell]
/// impl MyStruct {
///     pub fn new() -> Self {
///         Self {}
///     }
///
///     pub fn i_take_hi_closure<F>(&self, mut arg: F) -> i32
///     where
///         F: hiarc::HiFnOnce<i32, i32>,
///     {
///         arg.call_once(2)
///     }
/// }
///
/// let s = MyStruct::new();
/// let a = 3;
/// let a = &a;
/// assert!(
///     s.i_take_hi_closure({
///         #[derive(hiarc_macro::Hiarc)]
///         pub struct WrittenOutHiClosure<'a> {
///             a: &'a i32,
///         }
///         unsafe impl<'a> hiarc::HiFnOnceBase<i32, i32> for WrittenOutHiClosure<'a> {
///             fn call_once(self, arg: i32) -> i32 {
///                 let a = self.a;
///                 arg + *a + 1
///             }
///         }
///         unsafe impl<'a> hiarc::HiFnOnce<i32, i32> for WrittenOutHiClosure<'a> {}
///         WrittenOutHiClosure { a: &a }
///     }) == 6
/// );
/// assert!(s.i_take_hi_closure(
///     hiarc::hi_closure!([a: &i32], |arg: i32| -> i32 { arg + *a + 1 })
/// ) == 6);
/// ```
///
#[proc_macro]
pub fn hi_closure(item: TokenStream) -> TokenStream {
    hi_closure_impl(item)
}
