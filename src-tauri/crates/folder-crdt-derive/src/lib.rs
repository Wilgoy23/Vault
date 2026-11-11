//! `#[derive(Merge)]` and `#[derive(Record)]` for structs built from
//! folder-crdt's field types. Use them through `folder_crdt`, which
//! re-exports both beside the traits of the same names.
//!
//! ```ignore
//! use folder_crdt::{Counter, Hlc, Lww, Merge, Record};
//!
//! #[derive(Default, Merge, Record)]
//! #[record(clock = Hlc)]
//! struct Bookmark {
//!     title: Lww<String, Hlc>,
//!     url: Lww<String, Hlc>,
//!     // Opening a bookmark isn't an edit
//!     #[record(skip)]
//!     opened: Lww<u64, Hlc>,
//!     visits: Counter,
//! }
//! ```
//!
//! `Merge` merges every field with its own `Merge`. Each field is taken out
//! of the other copy with `std::mem::take`, so field types implement
//! `Default` (all of folder-crdt's do), and structs that zeroize on drop can
//! derive it too.
//!
//! `Record` makes `last_edit` the newest clock any field reports through
//! `folder_crdt::LastEdit`. Registers report their clock; counters, maxima,
//! sets, immutable values and unknown fields report none, so using a
//! credential or bumping a count never makes a record look edited. Mark a
//! register that shouldn't count either with `#[record(skip)]`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, Data, DeriveInput, Error, Fields, Index, Member, Type};

#[proc_macro_derive(Merge)]
pub fn derive_merge(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_merge(&input).unwrap_or_else(Error::into_compile_error).into()
}

#[proc_macro_derive(Record, attributes(record))]
pub fn derive_record(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_record(&input).unwrap_or_else(Error::into_compile_error).into()
}

struct Field {
    member: Member,
    ty: Type,
    skip: bool,
}

fn fields(input: &DeriveInput, derive: &str) -> syn::Result<Vec<Field>> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new(
            input.ident.span(),
            format!("#[derive({derive})] only supports structs: an enum has no field-by-field merge"),
        ));
    };
    let list: Vec<&syn::Field> = match &data.fields {
        Fields::Named(f) => f.named.iter().collect(),
        Fields::Unnamed(f) => f.unnamed.iter().collect(),
        Fields::Unit => Vec::new(),
    };
    list.into_iter()
        .enumerate()
        .map(|(i, f)| {
            let member = match &f.ident {
                Some(ident) => Member::Named(ident.clone()),
                None => Member::Unnamed(Index { index: i as u32, span: f.span() }),
            };
            Ok(Field { member, ty: f.ty.clone(), skip: field_skipped(f)? })
        })
        .collect()
}

/// Whether a field carries `#[record(skip)]`.
fn field_skipped(field: &syn::Field) -> syn::Result<bool> {
    let mut skip = false;
    for attr in field.attrs.iter().filter(|a| a.path().is_ident("record")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                skip = true;
                Ok(())
            } else {
                Err(meta.error("expected `skip`"))
            }
        })?;
    }
    Ok(skip)
}

/// The type named by the container's `#[record(clock = …)]`.
fn clock_type(input: &DeriveInput) -> syn::Result<Type> {
    let mut clock = None;
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("record")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("clock") {
                clock = Some(meta.value()?.parse::<Type>()?);
                Ok(())
            } else {
                Err(meta.error("expected `clock = Type`"))
            }
        })?;
    }
    clock.ok_or_else(|| {
        Error::new(
            input.ident.span(),
            "#[derive(Record)] needs the clock type its fields are stamped with: #[record(clock = Hlc)]",
        )
    })
}

/// Generics with `bound` added for each field type, when the struct has
/// type parameters (concrete structs need no bounds, and adding them would
/// expose private field types in a public impl).
fn generics_with(input: &DeriveInput, fields: &[&Field], bound: Tokens) -> syn::Generics {
    let mut generics = input.generics.clone();
    if generics.type_params().next().is_some() {
        let clause = generics.make_where_clause();
        for f in fields {
            let ty = &f.ty;
            clause.predicates.push(syn::parse_quote!(#ty: #bound));
        }
    }
    generics
}

fn expand_merge(input: &DeriveInput) -> syn::Result<Tokens> {
    let fields = fields(input, "Merge")?;
    let all: Vec<&Field> = fields.iter().collect();
    let generics = generics_with(
        input,
        &all,
        quote!(::folder_crdt::Merge + ::core::default::Default),
    );
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let name = &input.ident;
    let members = fields.iter().map(|f| &f.member);
    let other = if fields.is_empty() { quote!(_other) } else { quote!(mut other) };
    Ok(quote! {
        impl #impl_generics ::folder_crdt::Merge for #name #ty_generics #where_clause {
            fn merge(&mut self, #other: Self) {
                #(
                    ::folder_crdt::Merge::merge(
                        &mut self.#members,
                        ::core::mem::take(&mut other.#members),
                    );
                )*
            }
        }
    })
}

fn expand_record(input: &DeriveInput) -> syn::Result<Tokens> {
    let clock = clock_type(input)?;
    let fields = fields(input, "Record")?;
    let counted: Vec<&Field> = fields.iter().filter(|f| !f.skip).collect();
    let generics = generics_with(input, &counted, quote!(::folder_crdt::LastEdit<#clock>));
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let name = &input.ident;
    let members = counted.iter().map(|f| &f.member);
    Ok(quote! {
        impl #impl_generics ::folder_crdt::Record for #name #ty_generics #where_clause {
            type Clock = #clock;

            fn last_edit(&self) -> ::core::option::Option<#clock> {
                let mut newest: ::core::option::Option<#clock> = ::core::option::Option::None;
                #(
                    if let ::core::option::Option::Some(c) =
                        ::folder_crdt::LastEdit::<#clock>::last_edit(&self.#members)
                    {
                        if newest.as_ref().map_or(true, |n| c > *n) {
                            newest = ::core::option::Option::Some(c);
                        }
                    }
                )*
                newest
            }
        }
    })
}
