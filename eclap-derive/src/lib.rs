//! Derive macros for `eclap::Args` and `eclap::Parser`.

use heck::ToKebabCase;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::ext::IdentExt;
use syn::meta::ParseNestedMeta;
use syn::{
    Attribute, Data, DataEnum, DataStruct, DeriveInput, Error, Expr, ExprLit, Field, Fields,
    GenericArgument, Ident, Lit, LitChar, LitStr, Meta, PathArguments, Result, Token, Type,
    parse_macro_input,
};

#[proc_macro_derive(Args, attributes(eclap, arg, command))]
pub fn derive_args(input: TokenStream) -> TokenStream {
    expand(parse_macro_input!(input as DeriveInput), false)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Parser, attributes(eclap, arg, command))]
pub fn derive_parser(input: TokenStream) -> TokenStream {
    expand(parse_macro_input!(input as DeriveInput), true)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand(input: DeriveInput, parser: bool) -> Result<TokenStream2> {
    reject(
        &input.attrs,
        "eclap",
        "`#[eclap]` is not supported on types; put `ns` and `ns_sep` on the field that holds this type",
    )?;
    reject(&input.attrs, "arg", "`#[arg]` is not supported on types")?;
    let command = if parser {
        forwarded(&input.attrs, "command", None, |name| match name {
            "version" => Some(quote!(.version(::core::env!("CARGO_PKG_VERSION")))),
            "author" => Some(quote!(.author(::core::env!("CARGO_PKG_AUTHORS")))),
            "about" => Some(quote!(.about(::core::env!("CARGO_PKG_DESCRIPTION")))),
            _ => None,
        })?
    } else {
        reject(
            &input.attrs,
            "command",
            "`#[command]` is only supported with `#[derive(eclap::Parser)]`",
        )?;
        Vec::new()
    };

    let (augment, from_matches) = match &input.data {
        Data::Struct(data) => struct_bodies(&input.ident, data)?,
        Data::Enum(data) if !parser => enum_bodies(&input, data)?,
        Data::Enum(_) => {
            return Err(Error::new(
                input.ident.span(),
                "`eclap::Parser` requires a struct with named fields",
            ));
        }
        Data::Union(data) => {
            return Err(Error::new(
                data.union_token.span,
                "eclap derives cannot be applied to unions",
            ));
        }
    };

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let parser_impls = parser.then(|| {
        let (about, long_about) = help(&input.attrs);
        let command = command.iter().map(|(_, call)| call);
        let base = quote! {
            ::eclap::clap::Command::new(::core::env!("CARGO_PKG_NAME"))
                .about(#about)
                .long_about(#long_about)
                #(#command)*
        };
        quote! {
            impl #impl_generics ::eclap::clap::CommandFactory for #name #ty_generics #where_clause {
                fn command() -> ::eclap::clap::Command {
                    <Self as ::eclap::clap::Args>::augment_args(#base)
                }

                fn command_for_update() -> ::eclap::clap::Command {
                    <Self as ::eclap::clap::Args>::augment_args_for_update(#base)
                }
            }

            impl #impl_generics ::eclap::clap::Parser for #name #ty_generics #where_clause {}

            #[allow(dead_code)]
            impl #impl_generics #name #ty_generics #where_clause {
                pub fn parse() -> Self {
                    <Self as ::eclap::clap::Parser>::parse()
                }

                pub fn try_parse() -> ::core::result::Result<Self, ::eclap::clap::Error> {
                    <Self as ::eclap::clap::Parser>::try_parse()
                }

                pub fn parse_from<__EclapIter, __EclapItem>(iter: __EclapIter) -> Self
                where
                    __EclapIter: ::core::iter::IntoIterator<Item = __EclapItem>,
                    __EclapItem: ::core::convert::Into<::std::ffi::OsString> + ::core::clone::Clone,
                {
                    <Self as ::eclap::clap::Parser>::parse_from(iter)
                }

                pub fn try_parse_from<__EclapIter, __EclapItem>(
                    iter: __EclapIter,
                ) -> ::core::result::Result<Self, ::eclap::clap::Error>
                where
                    __EclapIter: ::core::iter::IntoIterator<Item = __EclapItem>,
                    __EclapItem: ::core::convert::Into<::std::ffi::OsString> + ::core::clone::Clone,
                {
                    <Self as ::eclap::clap::Parser>::try_parse_from(iter)
                }
            }
        }
    });

    Ok(quote! {
        impl #impl_generics ::eclap::Args for #name #ty_generics #where_clause {
            #[allow(unused_variables, unused_parens)]
            fn augment(cmd: ::eclap::clap::Command, scope: &::eclap::Scope) -> ::eclap::clap::Command {
                #augment
            }

            #[allow(unused_variables)]
            fn from_matches(
                matches: &::eclap::clap::ArgMatches,
                scope: &::eclap::Scope,
            ) -> ::core::result::Result<Self, ::eclap::clap::Error> {
                #from_matches
            }
        }

        impl #impl_generics ::eclap::clap::FromArgMatches for #name #ty_generics #where_clause {
            fn from_arg_matches(
                matches: &::eclap::clap::ArgMatches,
            ) -> ::core::result::Result<Self, ::eclap::clap::Error> {
                <Self as ::eclap::Args>::from_matches(matches, &::eclap::Scope::root())
            }

            fn update_from_arg_matches(
                &mut self,
                matches: &::eclap::clap::ArgMatches,
            ) -> ::core::result::Result<(), ::eclap::clap::Error> {
                *self = <Self as ::eclap::clap::FromArgMatches>::from_arg_matches(matches)?;
                ::core::result::Result::Ok(())
            }
        }

        impl #impl_generics ::eclap::clap::Args for #name #ty_generics #where_clause {
            fn augment_args(cmd: ::eclap::clap::Command) -> ::eclap::clap::Command {
                <Self as ::eclap::Args>::augment(cmd, &::eclap::Scope::root())
            }

            fn augment_args_for_update(cmd: ::eclap::clap::Command) -> ::eclap::clap::Command {
                <Self as ::eclap::clap::Args>::augment_args(cmd)
            }
        }

        #parser_impls
    })
}

/// Calls `f` for every key of every `#[eclap(...)]` attribute.
fn eclap_meta(attrs: &[Attribute], mut f: impl FnMut(ParseNestedMeta) -> Result<()>) -> Result<()> {
    for attr in attrs.iter().filter(|a| a.path().is_ident("eclap")) {
        attr.parse_nested_meta(&mut f)?;
    }
    Ok(())
}

fn reject(attrs: &[Attribute], name: &str, message: &str) -> Result<()> {
    match attrs.iter().find(|a| a.path().is_ident(name)) {
        Some(attr) => Err(Error::new_spanned(attr, message)),
        None => Ok(()),
    }
}

/// Every key of the `#[<attr>(...)]` attributes as `(name, ".name(args)")`: `key = expr` and
/// `key(args)` are forwarded verbatim, bare keys are resolved by `bare`.
fn forwarded(
    attrs: &[Attribute],
    attr: &str,
    value_ty: Option<(&Type, bool)>,
    bare: impl Fn(&str) -> Option<TokenStream2>,
) -> Result<Vec<(String, TokenStream2)>> {
    let mut calls = Vec::new();
    for attribute in attrs.iter().filter(|a| a.path().is_ident(attr)) {
        attribute.parse_nested_meta(|meta| {
            let method = meta
                .path
                .get_ident()
                .ok_or_else(|| meta.error("expected a method name"))?
                .clone();
            let name = method.unraw().to_string();
            if attr == "arg" && (name == "id" || name == "name") {
                return Err(meta.error(
                    "eclap derives the id from the field path; use `long = \"...\"` to rename the flag",
                ));
            }
            let call = if meta.input.peek(Token![=]) {
                let value: Expr = meta.value()?.parse()?;
                if let (true, Some((ty, value_enum))) = (name == "default_value_t", value_ty) {
                    let text = default_text(value_enum);
                    quote! {
                        .default_value({
                            let value: #ty = #value;
                            #text
                        })
                    }
                } else {
                    quote!(.#method(#value))
                }
            } else if meta.input.peek(syn::token::Paren) {
                let args;
                syn::parenthesized!(args in meta.input);
                let args: TokenStream2 = args.parse()?;
                quote!(.#method(#args))
            } else {
                bare(&name)
                    .ok_or_else(|| meta.error(format!("attribute `{name}` requires a value")))?
            };
            calls.push((name, call));
            Ok(())
        })?;
    }
    Ok(calls)
}

/// Text of the default `value`: the possible-value name for `ValueEnum` types, else `Display`.
fn default_text(value_enum: bool) -> TokenStream2 {
    if value_enum {
        quote! {
            ::eclap::clap::ValueEnum::to_possible_value(&value)
                .expect("eclap: skipped ValueEnum variant used as a default")
                .get_name()
                .to_owned()
        }
    } else {
        quote!(::std::string::ToString::to_string(&value))
    }
}

/// Whether `#[arg(...)]` contains the bare marker `value_enum`.
fn has_value_enum(attrs: &[Attribute]) -> bool {
    let mut value_enum = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("arg")) {
        // Syntax errors are reported by `forwarded`.
        let _ = attr.parse_nested_meta(|meta| {
            value_enum |= meta.path.is_ident("value_enum");
            if meta.input.peek(Token![=]) {
                meta.value()?.parse::<Expr>()?;
            } else if meta.input.peek(syn::token::Paren) {
                let args;
                syn::parenthesized!(args in meta.input);
                args.parse::<TokenStream2>()?;
            }
            Ok(())
        });
    }
    value_enum
}

fn kebab(ident: &Ident) -> String {
    ident.unraw().to_string().to_kebab_case()
}

fn is_bool(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.qself.is_none() && p.path.is_ident("bool"))
}

/// `T` of `wrapper<T>`, matched on the last path segment.
fn generic_inner<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != wrapper {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

/// `(help, long_help)` as `Option<&'static str>` tokens, from doc comments.
fn help(attrs: &[Attribute]) -> (TokenStream2, TokenStream2) {
    let mut paragraphs: Vec<Vec<String>> = vec![vec![]];
    for attr in attrs.iter().filter(|a| a.path().is_ident("doc")) {
        let Meta::NameValue(nv) = &attr.meta else {
            continue;
        };
        let Expr::Lit(ExprLit {
            lit: Lit::Str(doc), ..
        }) = &nv.value
        else {
            continue;
        };
        for line in doc.value().split('\n').map(str::trim) {
            if !line.is_empty() {
                paragraphs.last_mut().unwrap().push(line.to_owned());
            } else if !paragraphs.last().unwrap().is_empty() {
                paragraphs.push(vec![]);
            }
        }
    }
    if paragraphs.last().unwrap().is_empty() {
        paragraphs.pop();
    }
    let paragraphs: Vec<String> = paragraphs.iter().map(|p| p.join(" ")).collect();
    let some = |s: String| quote!(::core::option::Option::Some(#s));
    let none = || quote!(::core::option::Option::None);
    match paragraphs.as_slice() {
        [] => (none(), none()),
        [only] => (some(only.clone()), none()),
        [first, ..] => (some(first.clone()), some(paragraphs.join("\n\n"))),
    }
}

enum Kind {
    Value,
    Flatten,
    Subcommand,
    Nested {
        segment: LitStr,
        separator: TokenStream2,
    },
}

fn field_kind(field: &Field, ident: &Ident) -> Result<Kind> {
    let (mut flatten, mut subcommand, mut nested) = (false, false, false);
    let mut segment = None;
    let mut separator = quote!(::core::option::Option::None);
    eclap_meta(&field.attrs, |meta| {
        if meta.path.is_ident("flatten") {
            flatten = true;
        } else if meta.path.is_ident("subcommand") {
            subcommand = true;
        } else if meta.path.is_ident("nested") {
            nested = true;
        } else if meta.path.is_ident("ns") {
            nested = true;
            segment = Some(meta.value()?.parse::<LitStr>()?);
        } else if meta.path.is_ident("ns_sep") {
            nested = true;
            let c: LitChar = meta.value()?.parse()?;
            if c.value() == '=' || c.value().is_whitespace() {
                return Err(Error::new(c.span(), "`ns_sep` cannot be `=` or whitespace"));
            }
            separator = quote!(::core::option::Option::Some(#c));
        } else {
            return Err(meta.error(
                "unknown eclap attribute, expected `nested`, `ns`, `ns_sep`, `flatten` or `subcommand`",
            ));
        }
        Ok(())
    })?;

    let kinds = [flatten, subcommand, nested];
    if kinds.iter().filter(|&&k| k).count() > 1 {
        return Err(Error::new_spanned(
            field,
            "`flatten`, `subcommand` and `nested`/`ns`/`ns_sep` are mutually exclusive",
        ));
    }
    if kinds.contains(&true) {
        reject(
            &field.attrs,
            "arg",
            "`#[arg]` is only supported on plain value fields",
        )?;
    }
    Ok(if flatten {
        Kind::Flatten
    } else if subcommand {
        Kind::Subcommand
    } else if nested {
        Kind::Nested {
            segment: segment.unwrap_or_else(|| LitStr::new(&kebab(ident), ident.span())),
            separator,
        }
    } else {
        Kind::Value
    })
}

fn struct_bodies(name: &Ident, data: &DataStruct) -> Result<(TokenStream2, TokenStream2)> {
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new(
            name.span(),
            "eclap derives require a struct with named fields",
        ));
    };

    let mut augment = Vec::new();
    let mut init = Vec::new();
    let mut subcommands = 0;
    for field in &fields.named {
        reject(
            &field.attrs,
            "command",
            "`#[command]` is only supported on the type, with `#[derive(eclap::Parser)]`",
        )?;
        let ident = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let flag = LitStr::new(&kebab(ident), ident.span());

        let (register, read) = match field_kind(field, ident)? {
            Kind::Nested { segment, separator } => {
                let child = quote!(&scope.child(#segment, #separator));
                (
                    quote!(<#ty as ::eclap::Args>::augment(cmd, #child)),
                    quote!(<#ty as ::eclap::Args>::from_matches(matches, #child)?),
                )
            }
            Kind::Flatten => (
                quote! {{
                    scope.unconditional(#flag);
                    <#ty as ::eclap::clap::Args>::augment_args(cmd)
                }},
                quote!(<#ty as ::eclap::clap::FromArgMatches>::from_arg_matches(matches)?),
            ),
            Kind::Subcommand => {
                subcommands += 1;
                if subcommands > 1 {
                    return Err(Error::new_spanned(
                        field,
                        "at most one `subcommand` field is allowed",
                    ));
                }
                if let Some(inner) = generic_inner(ty, "Option") {
                    (
                        quote! {{
                            scope.unconditional(#flag);
                            <#inner as ::eclap::clap::Subcommand>::augment_subcommands(cmd)
                        }},
                        quote! {
                            if matches
                                .subcommand_name()
                                .is_some_and(<#inner as ::eclap::clap::Subcommand>::has_subcommand)
                            {
                                ::core::option::Option::Some(
                                    <#inner as ::eclap::clap::FromArgMatches>::from_arg_matches(matches)?,
                                )
                            } else {
                                ::core::option::Option::None
                            }
                        },
                    )
                } else {
                    (
                        quote! {{
                            scope.unconditional(#flag);
                            <#ty as ::eclap::clap::Subcommand>::augment_subcommands(cmd)
                                .subcommand_required(true)
                                .arg_required_else_help(true)
                        }},
                        quote!(<#ty as ::eclap::clap::FromArgMatches>::from_arg_matches(matches)?),
                    )
                }
            }
            Kind::Value => value_field(field, &flag)?,
        };
        augment.push(quote!(let cmd = #register;));
        init.push(quote!(#ident: #read));
    }

    Ok((
        quote!(#(#augment)* cmd),
        quote!(::core::result::Result::Ok(Self { #(#init),* })),
    ))
}

/// `(register, read)` tokens of a plain value field.
fn value_field(field: &Field, flag: &LitStr) -> Result<(TokenStream2, TokenStream2)> {
    let ty = &field.ty;
    let (help, long_help) = help(&field.attrs);
    let option = generic_inner(ty, "Option");
    let vec = generic_inner(ty, "Vec");
    let value_ty = option.or(vec).unwrap_or(ty);
    let short = flag.value().chars().next().unwrap_or_default();
    let value_enum = has_value_enum(&field.attrs);
    let text = default_text(value_enum);
    let calls = forwarded(
        &field.attrs,
        "arg",
        Some((value_ty, value_enum)),
        |name| match name {
            "short" => Some(quote!(.short(#short))),
            "env" => Some(quote!(.env(scope.env(#flag)))),
            "default_value_t" => Some(quote! {
                .default_value({
                    let value = <#value_ty as ::core::default::Default>::default();
                    #text
                })
            }),
            "long" | "value_enum" | "value_parser" | "action" => Some(quote!()),
            _ => None,
        },
    )?;
    let forwarded_required = calls.iter().any(|(name, _)| name == "required");
    let custom_parser = calls
        .iter()
        .any(|(name, call)| name == "value_parser" && !call.is_empty());
    let calls = calls.iter().map(|(_, call)| call);

    let parsed = if custom_parser {
        quote!()
    } else {
        quote!(.value_parser(::eclap::clap::value_parser!(#value_ty)))
    };
    let (arg, read) = if is_bool(ty) {
        (
            quote!(#parsed.action(::eclap::clap::ArgAction::SetTrue)),
            quote!(scope.flag(matches, #flag)?),
        )
    } else if option.is_some() {
        (
            quote!(#parsed.action(::eclap::clap::ArgAction::Set)),
            quote!(scope.one::<#value_ty>(matches, #flag)?),
        )
    } else if vec.is_some() {
        (
            quote!(#parsed.action(::eclap::clap::ArgAction::Append)),
            quote!(scope.many::<#value_ty>(matches, #flag)?),
        )
    } else {
        (
            quote!(#parsed.action(::eclap::clap::ArgAction::Set)),
            quote!(scope.require::<#value_ty>(matches, #flag)?),
        )
    };
    let required = (option.is_none() && vec.is_none() && !is_bool(ty) && !forwarded_required)
        .then(|| quote!(let arg = scope.required_if_needed(arg);));
    let register = quote! {{
        let arg = scope.arg(#flag, #help, #long_help) #arg #(#calls)*;
        #required
        scope.add(cmd, arg)
    }};
    Ok((register, read))
}

fn enum_bodies(input: &DeriveInput, data: &DataEnum) -> Result<(TokenStream2, TokenStream2)> {
    if data.variants.is_empty() {
        return Err(Error::new(
            input.ident.span(),
            "`eclap::Args` requires an enum with at least one variant",
        ));
    }

    let (help, long_help) = help(&input.attrs);
    let mut values = Vec::new();
    let mut variants = Vec::new();
    let mut arms = Vec::new();
    for variant in &data.variants {
        reject_variant_attrs(&variant.attrs)?;
        let ident = &variant.ident;
        let value = ident.unraw().to_string();
        let segment = kebab(ident);
        match &variant.fields {
            Fields::Unit => arms.push(quote!(#value => ::core::result::Result::Ok(Self::#ident))),
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field = &fields.unnamed[0];
                reject_variant_attrs(&field.attrs)?;
                let ty = &field.ty;
                variants.push(quote! {
                    let cmd = scope.augment_variant::<#ty>(cmd, &tag_id, #value, #segment);
                });
                arms.push(quote! {
                    #value => ::core::result::Result::Ok(Self::#ident(<#ty as ::eclap::Args>::from_matches(
                        matches,
                        &scope.variant(&tag_id, #value, #segment),
                    )?))
                });
            }
            _ => {
                return Err(Error::new_spanned(
                    variant,
                    "`eclap::Args` supports only unit and single-field tuple variants",
                ));
            }
        }
        values.push(value);
    }

    let augment = quote! {
        let tag_id = scope.id("type");
        let cmd = scope.add(cmd, scope.required(
            scope.arg("type", #help, #long_help).env(scope.env("type")).value_parser(
                ::eclap::clap::builder::PossibleValuesParser::new([#(#values),*]),
            ),
        ));
        #(#variants)*
        cmd
    };
    let from_matches = quote! {
        let tag_id = scope.id("type");
        match scope.require::<::std::string::String>(matches, "type")?.as_str() {
            #(#arms,)*
            other => ::core::result::Result::Err(::eclap::clap::Error::raw(
                ::eclap::clap::error::ErrorKind::InvalidValue,
                ::std::format!("invalid value `{other}` for --{tag_id}\n"),
            )),
        }
    };
    Ok((augment, from_matches))
}

fn reject_variant_attrs(attrs: &[Attribute]) -> Result<()> {
    for name in ["eclap", "arg", "command"] {
        reject(
            attrs,
            name,
            "eclap attributes are not supported on enum variants",
        )?;
    }
    Ok(())
}
