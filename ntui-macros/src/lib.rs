use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{FnArg, ItemFn, Type, parse_macro_input};

// ---------- #[component] ----------

/// Turns `fn Name(props: &NameProps, hooks: &mut Hooks) -> Element { .. }`
/// (or the props-less form `fn Name(hooks: &mut Hooks) -> Element`) into a
/// unit struct implementing ntui::Component.
#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let f = parse_macro_input!(item as ItemFn);
    if !f.sig.generics.params.is_empty() || f.sig.asyncness.is_some() {
        return syn::Error::new_spanned(
            &f.sig,
            "#[component] functions cannot be generic or async",
        )
        .to_compile_error()
        .into();
    }
    let name = &f.sig.ident;
    let vis = &f.vis;
    let attrs = &f.attrs;
    let body = &f.block;
    let inputs: Vec<&FnArg> = f.sig.inputs.iter().collect();
    let (props_ty, props_pat, hooks_pat): (Type, Box<syn::Pat>, Box<syn::Pat>) =
        match inputs.as_slice() {
            [FnArg::Typed(p), FnArg::Typed(h)] => {
                let Type::Reference(r) = &*p.ty else {
                    return syn::Error::new_spanned(
                        &p.ty,
                        "props argument must be a reference (&MyProps)",
                    )
                    .to_compile_error()
                    .into();
                };
                ((*r.elem).clone(), p.pat.clone(), h.pat.clone())
            }
            [FnArg::Typed(h)] => (
                syn::parse_quote!(()),
                syn::parse_quote!(_props),
                h.pat.clone(),
            ),
            _ => {
                return syn::Error::new_spanned(
                &f.sig,
                "#[component] fn must take (props: &P, hooks: &mut Hooks) or (hooks: &mut Hooks)",
            )
            .to_compile_error()
            .into();
            }
        };
    quote! {
        #[allow(non_camel_case_types)]
        #(#attrs)*
        #vis struct #name;

        impl ::ntui::Component for #name {
            type Props = #props_ty;
            fn render(#props_pat: &#props_ty, #hooks_pat: &mut ::ntui::Hooks) -> ::ntui::Element #body
        }
    }
    .into()
}

// ---------- element! ----------

struct ElementNode {
    name: syn::Ident,
    props: Vec<(syn::Ident, syn::Expr)>,
    key: Option<syn::Expr>,
    children: Vec<Child>,
}

enum Child {
    Node(ElementNode),
    Splice(syn::Expr),
}

impl Parse for ElementNode {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;
        let mut props = Vec::new();
        let mut key = None;
        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            while !content.is_empty() {
                let pname: syn::Ident = content.parse()?;
                content.parse::<syn::Token![:]>()?;
                let expr: syn::Expr = content.parse()?;
                if pname == "key" {
                    key = Some(expr);
                } else {
                    props.push((pname, expr));
                }
                if content.peek(syn::Token![,]) {
                    content.parse::<syn::Token![,]>()?;
                }
            }
        }
        let mut children = Vec::new();
        if input.peek(syn::token::Brace) {
            let content;
            syn::braced!(content in input);
            while !content.is_empty() {
                if content.peek(syn::Token![#]) {
                    content.parse::<syn::Token![#]>()?;
                    let inner;
                    syn::parenthesized!(inner in content);
                    children.push(Child::Splice(inner.parse()?));
                } else {
                    children.push(Child::Node(content.parse()?));
                }
            }
        }
        Ok(ElementNode {
            name,
            props,
            key,
            children,
        })
    }
}

/// JSX-alike: `element! { View(gap: 1) { Text(content: "hi") #(iter) } }`.
/// Hosts: View, Text, Fragment, ContextProvider(value: ..). Anything else is a
/// component; its props type is `{Name}Props` by convention (Default-filled).
///
/// Identifiers prefixed with `__` are reserved inside `element!` expansions.
///
/// The expansion emits fully-qualified `::ntui::` paths (proc macros cannot use
/// `$crate`), so the library must be reachable as `::ntui` — i.e. depended on as
/// `ntui`; renaming the dependency breaks codegen.
#[proc_macro]
pub fn element(input: TokenStream) -> TokenStream {
    let node = parse_macro_input!(input as ElementNode);
    gen_node(&node).into()
}

/// True for an integer literal with no explicit type suffix (`1`, not
/// `1u16` or `1.0`), including a leading-minus form (`-1`).
fn is_unsuffixed_int_lit(expr: &syn::Expr) -> bool {
    fn lit_is_bare_int(lit: &syn::Lit) -> bool {
        matches!(lit, syn::Lit::Int(i) if i.suffix().is_empty())
    }
    match expr {
        syn::Expr::Lit(syn::ExprLit { lit, .. }) => lit_is_bare_int(lit),
        syn::Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr,
            ..
        }) => is_unsuffixed_int_lit(expr),
        syn::Expr::Paren(syn::ExprParen { expr, .. }) => is_unsuffixed_int_lit(expr),
        _ => false,
    }
}

fn gen_node(node: &ElementNode) -> proc_macro2::TokenStream {
    let name = &node.name;
    let fields = node.props.iter().map(|(k, v)| {
        // Unsuffixed integer literals (e.g. `gap: 1`) must be emitted without
        // an `Into::into` wrapper: routing them through a generic `Into<T>`
        // call makes the literal's type an unresolved trait-selection
        // variable, which falls back to `i32` before the `Into<u16>` (etc.)
        // bound is checked, breaking inference. Bare struct-literal field
        // assignment lets the field's concrete type flow directly to the
        // literal instead, so it infers correctly.
        if is_unsuffixed_int_lit(v) {
            quote! { #k: #v, }
        } else {
            quote! { #k: ::core::convert::Into::into(#v), }
        }
    });
    let children = gen_children(&node.children);
    let name_str = name.to_string();
    let base = match name_str.as_str() {
        "View" => quote! {
            {
                // The `..Default::default()` spread is structural (it lets
                // callers omit any subset of ViewProps fields), even when a
                // particular `element!` call happens to specify every field.
                #[allow(clippy::needless_update)]
                let __props = ::ntui::ViewProps { #(#fields)* ..::core::default::Default::default() };
                ::ntui::Element::view(__props, #children)
            }
        },
        "Text" => quote! {
            {
                #[allow(clippy::needless_update)]
                let __props = ::ntui::TextProps { #(#fields)* ..::core::default::Default::default() };
                ::ntui::Element::text(__props)
            }
        },
        "Fragment" => quote! { ::ntui::Element::fragment(#children) },
        "ContextProvider" => {
            let Some((_, v)) = node.props.iter().find(|(k, _)| k == "value") else {
                return syn::Error::new_spanned(
                    &node.name,
                    "ContextProvider requires a `value:` prop",
                )
                .to_compile_error();
            };
            quote! { ::ntui::Element::provider(#v, #children) }
        }
        _ => {
            if node.props.is_empty() {
                quote! { ::ntui::Element::component::<#name>(::core::default::Default::default()) }
            } else {
                let props_ty = format_ident!("{}Props", name);
                quote! {
                    {
                        // Same rationale as the View/Text branches above: the
                        // spread is structural, not redundant, even when a
                        // given call fills every field of `{Name}Props`.
                        #[allow(clippy::needless_update)]
                        let __props = #props_ty { #(#fields)* ..::core::default::Default::default() };
                        ::ntui::Element::component::<#name>(__props)
                    }
                }
            }
        }
    };
    match &node.key {
        Some(k) => quote! { #base.with_key(#k) },
        None => base,
    }
}

fn gen_children(children: &[Child]) -> proc_macro2::TokenStream {
    let stmts = children.iter().map(|c| match c {
        Child::Node(n) => {
            let t = gen_node(n);
            quote! { __children.push(#t); }
        }
        Child::Splice(e) => quote! { __children.extend(#e); },
    });
    quote! {{
        let mut __children: ::std::vec::Vec<::ntui::Element> = ::std::vec::Vec::new();
        #(#stmts)*
        __children
    }}
}
