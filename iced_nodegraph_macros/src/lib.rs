//! Proc-macros for `iced_nodegraph` style structs.
//!
//! `#[style]` turns a flat, concrete struct into the typestate form used by the
//! style system: one struct generic over `StyleMode`, with each field wrapped by
//! `S::Wrap<T>` (`Option<T>` for `Partial`, `T` for `Resolved`). It also emits
//! the mechanical glue: `Clone`/`Debug`/`PartialEq`, `Default` for the partial
//! overlay, builder setters, and `merge`/`resolve`/`merge_theme`.
//!
//! Requirements at the use site:
//! - `StyleMode`, `Partial`, `Resolved` must be in scope.
//! - The struct must provide `from_theme(&iced::Theme) -> Self` on its resolved
//!   form (`Name<Resolved>`); `merge_theme` calls it.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Fields, ItemStruct, parse_macro_input};

/// Expands a flat style struct into its `Partial`/`Resolved` typestate form.
/// See the crate docs for the generated items and use-site requirements.
#[proc_macro_attribute]
pub fn style(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);

    let vis = &input.vis;
    let name = &input.ident;
    let struct_attrs = &input.attrs;

    let named = match input.fields {
        Fields::Named(ref n) => &n.named,
        _ => {
            return syn::Error::new_spanned(
                &input.ident,
                "#[style] requires a struct with named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    let idents: Vec<_> = named.iter().map(|f| f.ident.clone().unwrap()).collect();
    let types: Vec<_> = named.iter().map(|f| f.ty.clone()).collect();

    // Struct field definitions, preserving per-field attrs and visibility.
    let field_defs = named.iter().map(|f| {
        let attrs = &f.attrs;
        let fvis = &f.vis;
        let id = f.ident.as_ref().unwrap();
        let ty = &f.ty;
        quote! { #(#attrs)* #fvis #id: <S as StyleMode>::Wrap<#ty> }
    });

    // Builder setters on the partial overlay.
    let setters = named.iter().map(|f| {
        let id = f.ident.as_ref().unwrap();
        let ty = &f.ty;
        quote! {
            #[doc = concat!("Sets `", stringify!(#id), "` (overrides inheritance).")]
            pub fn #id(mut self, value: impl ::core::convert::Into<#ty>) -> Self {
                self.#id = ::core::option::Option::Some(value.into());
                self
            }
        }
    });

    // Deduplicated field types for the trait-bound `where` clauses, so a type
    // used by several fields is not listed multiple times.
    let mut seen = std::collections::HashSet::new();
    let unique_types: Vec<_> = types
        .iter()
        .filter(|t| seen.insert(quote!(#t).to_string()))
        .collect();

    let expanded = quote! {
        #(#struct_attrs)*
        #vis struct #name<S: StyleMode = Partial> {
            #(#field_defs),*
        }

        #[automatically_derived]
        impl<S: StyleMode> ::core::clone::Clone for #name<S>
        where #( <S as StyleMode>::Wrap<#unique_types>: ::core::clone::Clone, )*
        {
            fn clone(&self) -> Self {
                Self { #( #idents: ::core::clone::Clone::clone(&self.#idents), )* }
            }
        }

        #[automatically_derived]
        impl<S: StyleMode> ::core::fmt::Debug for #name<S>
        where #( <S as StyleMode>::Wrap<#unique_types>: ::core::fmt::Debug, )*
        {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.debug_struct(::core::stringify!(#name))
                    #( .field(::core::stringify!(#idents), &self.#idents) )*
                    .finish()
            }
        }

        #[automatically_derived]
        impl<S: StyleMode> ::core::cmp::PartialEq for #name<S>
        where #( <S as StyleMode>::Wrap<#unique_types>: ::core::cmp::PartialEq, )*
        {
            fn eq(&self, other: &Self) -> bool {
                true #( && self.#idents == other.#idents )*
            }
        }

        #[automatically_derived]
        impl ::core::default::Default for #name<Partial> {
            fn default() -> Self {
                Self { #( #idents: ::core::option::Option::None, )* }
            }
        }

        impl #name<Partial> {
            /// Creates an empty overlay where every field inherits.
            pub fn new() -> Self {
                ::core::default::Default::default()
            }

            #(#setters)*

            /// Layers `self` over `other`; `self` wins where set. Stays partial.
            pub fn merge(&self, other: &Self) -> Self {
                Self {
                    #( #idents: ::core::clone::Clone::clone(&self.#idents)
                        .or_else(|| ::core::clone::Clone::clone(&other.#idents)), )*
                }
            }

            /// Resolves against an explicit base, filling unset fields from it.
            pub fn resolve(&self, base: &#name<Resolved>) -> #name<Resolved> {
                #name {
                    #( #idents: ::core::clone::Clone::clone(&self.#idents)
                        .unwrap_or_else(|| ::core::clone::Clone::clone(&base.#idents)), )*
                }
            }

            /// Resolves against the theme defaults (`Name::<Resolved>::from_theme`).
            pub fn merge_theme(&self, theme: &::iced::Theme) -> #name<Resolved> {
                self.resolve(&<#name<Resolved>>::from_theme(theme))
            }
        }
    };

    expanded.into()
}
