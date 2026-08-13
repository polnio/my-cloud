use derive_syn_parse::Parse;
use itertools::Itertools as _;
use proc_macro::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Ident, Token, punctuated::Punctuated};

#[derive(Parse)]
struct MethodDispatcher {
    vis: Option<Token![pub]>,
    ident: Ident,
    _colon: Token![:],
    #[call(Punctuated::parse_terminated)]
    methods: Punctuated<Ident, Token![,]>,
}

#[proc_macro]
pub fn method_dispatcher(input: TokenStream) -> TokenStream {
    let MethodDispatcher {
        vis,
        ident,
        _colon,
        methods,
    } = syn::parse_macro_input!(input as MethodDispatcher);

    let nah = format_ident!("NonAllowedHandler");

    let len = methods.len();
    let (mg, tg, ms, ts): (Vec<_>, Vec<_>, Vec<_>, Vec<_>) = (0..len)
        .map(|i| {
            (
                format_ident!("M{i}"),
                format_ident!("T{i}"),
                format_ident!("m{i}"),
                format_ident!("_t{i}"),
            )
        })
        .multiunzip();
    let nahm = (0..len).map(|_| &nah);
    let naht = (0..len).map(|_| quote!(()));
    let methods_str = methods.iter().map(ToString::to_string);

    let mhs = methods.iter().enumerate().map(|(i, method)| {
        let name = format_ident!("r#{}", method.to_string().to_lowercase());
        let t = Ident::new("T", Span::call_site().into());
        let tt = Ident::new("TT", Span::call_site().into());
        let handler = Ident::new("handler", Span::call_site().into());
        let (img, itg, ims, its): (Vec<_>, Vec<_>, Vec<_>, Vec<_>) = (0..len)
            .map(|j| {
                if i == j {
                    (
                        &t,
                        &tt,
                        quote!(#handler),
                        quote!(::std::marker::PhantomData),
                    )
                } else {
                    let img = &mg[j];
                    let itg = &tg[j];
                    let ims = &ms[j];
                    let its = &ts[j];
                    (img, itg, quote!(self.#ims), quote!(self.#its))
                }
            })
            .multiunzip();
        quote! {
            #vis fn #name<#t, #tt>(self, #handler: #t) -> Dispatcher<S, #(#img),*, #(#itg),*>
            where
                #t: ::axum::handler::Handler<#tt, S, Future = ::std::pin::Pin<Box<dyn Future<Output = ::axum::response::Response> + Send + 'static>>>,
            {
                Dispatcher {
                    #(#ms: #ims,)*
                    #(#ts: #its,)*
                    _s: self._s,
                }
            }
        }
    });

    quote! {
        #vis struct #ident<S, #(#mg),*, #(#tg),*> {
            #(#ms: #mg,)*
            #(#ts: ::std::marker::PhantomData<#tg>,)*
            _s: ::std::marker::PhantomData<S>,
        }
        impl<S> #ident<S, #(#nahm),*, #(#naht),*> {
            #vis const fn new() -> Self {
                Self {
                    #(#ms: #nah,)*
                    #(#ts: ::std::marker::PhantomData,)*
                    _s: ::std::marker::PhantomData,
                }
            }
        }
        impl<S, #(#mg),*, #(#tg),*> #ident<S, #(#mg),*, #(#tg),*> {
            #(#mhs)*
            #vis fn dispatch(self) -> ::axum::routing::MethodRouter<S>
            where
                S: Clone + Send + Sync + 'static,
                #(#mg: Clone + Send + Sync + 'static + ::axum::handler::Handler<#tg, S, Future = ::std::pin::Pin<Box<dyn Future<Output = ::axum::response::Response> + Send + 'static>>>,)*
                #(#tg: Send + Sync + 'static,)*
            {
                ::axum::routing::any::<_, (), _>(self)
            }
        }
        impl<S, #(#mg),*, #(#tg),*> ::axum::handler::Handler<(), S> for #ident<S, #(#mg),*, #(#tg),*>
        where
            S: Send + Sync + 'static,
            #(#mg: Clone + Send + Sync + 'static + ::axum::handler::Handler<#tg, S, Future = ::std::pin::Pin<Box<dyn Future<Output = ::axum::response::Response> + Send + 'static>>>,)*
            #(#tg: Send + Sync + 'static,)*
        {
            type Future = ::std::pin::Pin<Box<dyn Future<Output = ::axum::response::Response> + Send + 'static>>;

            fn call(self, req: ::axum::extract::Request, state: S) -> Self::Future {
                match req.method().as_str() {
                    #(#methods_str => ::axum::handler::Handler::call(self.#ms, req, state),)*
                    _ => Box::pin(async move { axum::response::IntoResponse::into_response(::axum::http::StatusCode::METHOD_NOT_ALLOWED) }),
                }
            }
        }

        impl<S, #(#mg),*, #(#tg),*> Clone for #ident<S, #(#mg),*, #(#tg),*>
        where
            #(#mg: Clone,)*
        {
            fn clone(&self) -> Self {
                Self {
                    #(#ms: self.#ms.clone(),)*
                    #(#ts: self.#ts,)*
                    _s: self._s,
                }
            }
        }

        #[derive(Clone)]
        #vis struct #nah;
        impl<T, S> ::axum::handler::Handler<T, S> for #nah {
            type Future = ::std::pin::Pin<Box<dyn Future<Output = ::axum::response::Response> + Send>>;

            fn call(self, _req: ::axum::extract::Request, _state: S) -> Self::Future {
                Box::pin(async move { axum::response::IntoResponse::into_response(::axum::http::StatusCode::METHOD_NOT_ALLOWED) })
            }
        }
    }
    .into()
}
