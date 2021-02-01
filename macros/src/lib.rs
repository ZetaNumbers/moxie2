use darling::FromMeta;
use proc_macro::TokenStream;
use quote::ToTokens;

mod local_slots {
    use crate::utils;
    use darling::FromMeta;
    use derive_syn_parse::Parse;
    use iroha::ToTokens;
    use proc_macro2::{Span, TokenStream as TokenStream2};
    use quote::{format_ident, quote, TokenStreamExt};
    use syn::spanned::Spanned;

    pub fn local_slots_impl(
        Args { namespace }: Args,
        mut input: utils::UniversalItemFn,
    ) -> syn::Result<Api> {
        use syn::visit_mut::VisitMut;
        return {
            let ctx = InterfaceTy::EnterCtx(Default::default()).into_mangled_ident();

            let (ty_def, init_fn_block): (TokenStream2, TokenStream2) =
                if let Some(block) = &mut input.block {
                    let mut expander = LocalSlotMacroExpander(Vec::new());
                    expander.visit_block_mut(block);
                    let (tys, exprs) = expander.iter_pair();
                    (
                        quote! { = (#(#tys,)*); },
                        quote! {
                            {
                                (#(#exprs,)*)
                            }
                        },
                    )
                } else {
                    (quote! {;}, quote! {;})
                };

            let type_name = InterfaceTy::Type(Default::default()).mangle_ident(&input.sig.ident);
            let init_fn_name =
                InterfaceTy::InitFn(Default::default()).mangle_ident(&input.sig.ident);
            input.sig.ident =
                InterfaceTy::EnterFn(Default::default()).mangle_ident(&input.sig.ident);

            input.sig.inputs.push({
                let ty: syn::Type = syn::parse2(quote! { #namespace :: #type_name })?;
                let out = syn::parse2(quote! { #ctx : &mut #ty })?;
                out
            });

            Ok(Api {
                ty: syn::parse2(quote! { type #type_name #ty_def })?,
                init_fn: syn::parse2(
                    quote! { fn #init_fn_name () -> #namespace :: #type_name #init_fn_block },
                )?,
                enter_fn: input,
            })
        };

        struct LocalSlotMacroExpander(Vec<(syn::Type, syn::Expr)>);

        impl LocalSlotMacroExpander {
            fn iter_pair<'a>(
                &'a self,
            ) -> (
                impl Iterator<Item = &syn::Type> + 'a,
                impl Iterator<Item = &syn::Expr> + 'a,
            ) {
                (self.0.iter().map(|p| &p.0), self.0.iter().map(|p| &p.1))
            }
        }

        impl syn::visit_mut::VisitMut for LocalSlotMacroExpander {
            fn visit_item_mut(&mut self, _: &mut syn::Item) {
                // ignore
            }
            fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
                match expr {
                    syn::Expr::Macro(syn::ExprMacro {
                        attrs,
                        mac: syn::Macro { path, tokens, .. },
                    }) if attrs.is_empty()
                        && match path.get_ident() {
                            Some(ident) => ident == "local_slot",
                            None => false,
                        } =>
                    {
                        *expr = || -> syn::Result<syn::ExprField> {
                            let syn::ExprCast { expr, ty, .. } =
                                syn::parse2::<syn::ExprCast>(std::mem::take(tokens))?;

                            // expand `local_slot!(...)` macro

                            let ctx =
                                InterfaceTy::EnterCtx(Default::default()).into_mangled_ident();
                            let i = syn::Index::from(self.0.len());
                            self.0.push((*ty, *expr));
                            syn::parse2(quote! { #ctx . #i })
                        }()
                        .map_err(syn::Error::into_compile_error)
                        .map_or_else(syn::Expr::Verbatim, syn::Expr::Field);
                    }
                    _ => (),
                }
                syn::visit_mut::visit_expr_mut(self, expr);
            }
        }
    }

    pub fn local_slots_interface_impl(
        InterfaceReq { interface, path }: InterfaceReq,
    ) -> syn::Result<syn::Path> {
        match path {
            Some(path) => interface.mangle_path(path),
            None => {
                let ident = interface.into_mangled_ident();
                Ok(syn::parse_quote!(#ident))
            }
        }
    }

    #[derive(Parse)]
    pub struct InterfaceReq {
        interface: InterfaceTy,
        #[peek(syn::Ident)]
        path: Option<syn::Path>,
    }

    #[derive(Debug, Clone, Copy, Parse, ToTokens)]
    pub enum InterfaceTy {
        #[peek(syn::Token![type], name = "Type")]
        Type(syn::Token![type]),
        #[peek(kw::init, name = "InitFn")]
        InitFn(kw::init),
        #[peek(kw::enter, name = "EnterFn")]
        EnterFn(kw::enter),
        #[peek(kw::context, name = "EnterCtx")]
        EnterCtx(kw::context),
    }

    impl quote::IdentFragment for InterfaceTy {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            match self {
                InterfaceTy::Type(_) => "type",
                InterfaceTy::InitFn(_) => "init",
                InterfaceTy::EnterFn(_) => "enter",
                InterfaceTy::EnterCtx(_) => "context",
            }
            .fmt(f)
        }

        fn span(&self) -> Option<Span> {
            Some(match self {
                InterfaceTy::Type(tk) => tk.span(),
                InterfaceTy::InitFn(tk) => tk.span(),
                InterfaceTy::EnterFn(tk) => tk.span(),
                InterfaceTy::EnterCtx(tk) => tk.span(),
            })
        }
    }

    impl InterfaceTy {
        pub fn into_mangled_ident(self) -> syn::Ident {
            const SUFFIX_UUID: &str = "e67dd0c1_f2a8_4161_aa1b_18cdaec4e496";
            format_ident!("{}_{}_{}", self, env!("CARGO_PKG_NAME"), SUFFIX_UUID)
        }

        fn mangle_ident(self, ident: &syn::Ident) -> syn::Ident {
            format_ident!("{}_{}", ident, self.into_mangled_ident())
        }

        pub fn mangle_path(self, mut path: syn::Path) -> syn::Result<syn::Path> {
            let span = path.span();
            let last_ident = &mut path
                .segments
                .last_mut()
                .ok_or_else(|| syn::Error::new(span, "Expected nonempty path"))?
                .ident;
            *last_ident = self.mangle_ident(last_ident);
            Ok(path)
        }
    }

    mod kw {
        syn::custom_keyword!(init);
        syn::custom_keyword!(enter);
        syn::custom_keyword!(context);
    }

    #[derive(FromMeta, Default)]
    pub struct Args {
        #[darling(default)]
        pub namespace: Namespace,
    }

    #[derive(Debug, Parse, ToTokens, PartialEq, Eq)]
    pub struct Api {
        ty: utils::UniversalItemType,
        init_fn: utils::UniversalItemFn,
        enter_fn: utils::UniversalItemFn,
    }

    #[derive(FromMeta)]
    pub enum Namespace {
        #[darling(rename = "Self")]
        SelfType,
        #[darling(rename = "self")]
        SelfModule,
    }

    impl Default for Namespace {
        fn default() -> Self {
            Namespace::SelfModule
        }
    }

    impl quote::ToTokens for Namespace {
        fn to_tokens(&self, tokens: &mut TokenStream2) {
            tokens.append(proc_macro2::Ident::new(
                match self {
                    Namespace::SelfType => "Self",
                    Namespace::SelfModule => "self",
                },
                Span::call_site(),
            ))
        }
    }
}

mod nested_slots {
    use crate::{local_slots::*, utils};
    use derive_syn_parse::Parse;
    use quote::quote;
    use syn::visit_mut::VisitMut;

    pub fn nested_slots_impl(args: Args, mut input: utils::UniversalItemFn) -> syn::Result<Api> {
        return {
            if let Some(block) = &mut input.block {
                NestMacroExpander.visit_block_mut(block);
            }
            local_slots_impl(args, input)
        };

        struct NestMacroExpander;

        impl VisitMut for NestMacroExpander {
            fn visit_item_mut(&mut self, _: &mut syn::Item) {
                // ignore
            }
            fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
                match expr {
                    syn::Expr::Macro(syn::ExprMacro {
                        attrs,
                        mac: syn::Macro { path, tokens, .. },
                    }) if attrs.is_empty()
                        && match path.get_ident() {
                            Some(ident) => ident == "nest",
                            None => false,
                        } =>
                    {
                        *expr = || -> syn::Result<syn::ExprCall> {
                            let mut call: Call = syn::parse2(std::mem::take(tokens))?;
                            // expand `nest!(...)` macro

                            let init_path = InterfaceTy::InitFn(Default::default())
                                .mangle_path(call.func.path.clone())?;
                            let type_path = InterfaceTy::Type(Default::default())
                                .mangle_path(call.func.path.clone())?;

                            call.func.path = InterfaceTy::EnterFn(Default::default())
                                .mangle_path(call.func.path)?;
                            call.args.push(syn::parse2(
                                quote! { &mut local_slot!(#init_path() as #type_path) },
                            )?);

                            syn::parse2(quote! { #call })
                        }()
                        .map_err(syn::Error::into_compile_error)
                        .map_or_else(syn::Expr::Verbatim, syn::Expr::Call);
                    }
                    _ => (),
                }
                syn::visit_mut::visit_expr_mut(self, expr);
            }
        }

        #[derive(Debug, Parse)]
        struct Call {
            #[call(syn::Attribute::parse_outer)]
            attrs: Vec<syn::Attribute>,
            func: syn::ExprPath,
            #[paren]
            paren_token: syn::token::Paren,
            #[inside(paren_token)]
            #[parse_terminated(syn::Expr::parse)]
            args: syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>,
        }

        impl quote::ToTokens for Call {
            fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
                self.attrs.iter().for_each(|attr| attr.to_tokens(tokens));
                self.func.to_tokens(tokens);
                self.paren_token
                    .surround(tokens, |tokens| self.args.to_tokens(tokens));
            }
        }
    }

    pub type Args = crate::local_slots::Args;
}

#[proc_macro_attribute]
pub fn local_slots(args: TokenStream, input: TokenStream) -> TokenStream {
    let attr_args: syn::AttributeArgs = syn::parse_macro_input!(args);

    let args = match local_slots::Args::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };

    let input: utils::UniversalItemFn = syn::parse_macro_input!(input);

    match local_slots::local_slots_impl(args, input) {
        Ok(api) => api.into_token_stream(),
        Err(error) => error.into_compile_error(),
    }
    .into()
}

#[proc_macro]
pub fn local_slots_interface(input: TokenStream) -> TokenStream {
    let input: local_slots::InterfaceReq = syn::parse_macro_input!(input);

    match local_slots::local_slots_interface_impl(input) {
        Ok(ident) => ident.into_token_stream(),
        Err(error) => error.into_compile_error(),
    }
    .into()
}

#[proc_macro_attribute]
pub fn nested_slots(args: TokenStream, input: TokenStream) -> TokenStream {
    let attr_args: syn::AttributeArgs = syn::parse_macro_input!(args);

    let args = match nested_slots::Args::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };

    let input: utils::UniversalItemFn = syn::parse_macro_input!(input);

    match nested_slots::nested_slots_impl(args, input) {
        Ok(api) => api.into_token_stream(),
        Err(error) => error.into_compile_error(),
    }
    .into()
}

mod utils {
    use derive_syn_parse::Parse;
    use iroha::ToTokens;
    use syn::punctuated::Punctuated;

    #[derive(Debug, Parse, ToTokens, PartialEq, Eq)]
    pub struct UniversalItemType {
        #[call(syn::Attribute::parse_outer)]
        pub attrs: Vec<syn::Attribute>,
        pub vis: syn::Visibility,
        pub type_token: syn::Token![type],
        pub ident: syn::Ident,
        pub generics: syn::Generics,
        #[peek(syn::Token![:])]
        pub bounds: Option<ColonBounds>,
        #[peek(syn::Token![=])]
        pub ty: Option<EqType>,
        pub semi_token: syn::Token![;],
    }
    #[derive(Debug, Parse, ToTokens, PartialEq, Eq)]
    pub struct EqType(pub syn::Token![=], pub syn::Type);

    #[derive(Debug, Parse, ToTokens, PartialEq, Eq)]
    pub struct ColonBounds(
        pub syn::Token![:],
        #[call(Punctuated::parse_separated_nonempty)]
        pub  Punctuated<syn::TypeParamBound, syn::Token![+]>,
    );

    #[derive(Debug, Parse, ToTokens, PartialEq, Eq)]
    pub struct UniversalItemFn {
        #[call(syn::Attribute::parse_outer)]
        pub attrs: Vec<syn::Attribute>,
        pub vis: syn::Visibility,
        pub defaultness: Option<syn::Token![default]>,
        pub sig: syn::Signature,
        #[peek(syn::token::Brace)]
        pub block: Option<syn::Block>,
        #[peek(syn::Token![;])]
        pub semi_token: Option<syn::Token![;]>,
    }
}

#[cfg(test)]
mod tests {

    use crate::{local_slots::local_slots_interface_impl, nested_slots::nested_slots_impl};

    use super::*;
    use local_slots::Api;

    macro_rules! compare_foos {
        ($generate_from:literal, $expected:literal) => {
            let generated: Api = nested_slots_impl(
                local_slots::Args::default(),
                syn::parse_str($generate_from).unwrap(),
            )
            .unwrap();
            let expected: Api = syn::parse_str(
                &format!($expected,
                    foo_type = get_interface!(type foo).get_ident().unwrap(),
                    foo_init = get_interface!(init foo).get_ident().unwrap(),
                    foo_enter = get_interface!(enter foo).get_ident().unwrap(),
                    context = get_interface!(context).get_ident().unwrap(),
                )
            ).unwrap();
            assert_eq!(generated, expected);
        };
    }

    macro_rules! get_interface {
        ($($tt:tt)*) => {
            local_slots_interface_impl(syn::parse_quote!($($tt)*)).unwrap()
        };
    }

    #[test]
    fn no_body() {
        compare_foos!(
            "
                fn foo();
            ",
            "
                type {foo_type};
                fn {foo_init}() -> self:: {foo_type};
                fn {foo_enter}({context}: &mut self:: {foo_type});
            "
        );
    }

    #[test]
    fn no_slot() {
        compare_foos!(
            "
                fn foo() {}
            ",
            "
                type {foo_type} = ();
                fn {foo_init}() -> self:: {foo_type} {{ () }}
                fn {foo_enter}({context}: &mut self:: {foo_type}) {{}}
            "
        );
    }

    #[test]
    fn one_local_slot() {
        compare_foos!(
            "
                fn foo() {
                    local_slot!(69 as i32);
                }
            ",
            "
                type {foo_type} = (i32,);
                fn {foo_init}() -> self:: {foo_type} {{ (69,) }}
                fn {foo_enter}({context}: &mut self:: {foo_type}) {{ {context}.0; }}
            "
        );
    }

    #[test]
    fn one_nested_slot() {
        compare_foos!(
            "
                fn foo() {
                    nest!(foo());
                }
            ",
            "
                type {foo_type} = ({foo_type},);
                fn {foo_init}() -> self:: {foo_type} {{ ({foo_init}(),) }}
                fn {foo_enter}({context}: &mut self:: {foo_type}) {{ {foo_enter}(&mut {context}.0); }}
            "
        );
    }
}
