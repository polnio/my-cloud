use axum::extract::Request;
use axum::handler::Handler;
use axum::http::StatusCode;
use axum::response::{IntoResponse as _, Response};
use axum::routing::MethodRouter;
use std::marker::PhantomData;
use std::pin::Pin;

#[derive(Clone)]
pub struct NonAllowedHandler;

pub struct Dispatcher<S, G, O, PF, GT, OT, PFT> {
    get: G,
    options: O,
    propfind: PF,
    _gt: PhantomData<GT>,
    _ot: PhantomData<OT>,
    _pft: PhantomData<PFT>,
    _s: PhantomData<S>,
}
impl<S> Dispatcher<S, NonAllowedHandler, NonAllowedHandler, NonAllowedHandler, (), (), ()> {
    pub fn new() -> Self {
        Self {
            get: NonAllowedHandler,
            options: NonAllowedHandler,
            propfind: NonAllowedHandler,
            _gt: PhantomData,
            _ot: PhantomData,
            _pft: PhantomData,
            _s: PhantomData,
        }
    }
}
impl<S, G, O, PF, GT, OT, PFT> Dispatcher<S, G, O, PF, GT, OT, PFT> {
    pub fn get<T, TT>(self, handler: T) -> Dispatcher<S, T, O, PF, TT, OT, PFT>
    where
        T: Handler<TT, S, Future = HandlerFuture>,
    {
        Dispatcher {
            get: handler,
            propfind: self.propfind,
            options: self.options,
            _gt: PhantomData,
            _ot: self._ot,
            _pft: self._pft,
            _s: self._s,
        }
    }
    pub fn options<T, TT>(self, handler: T) -> Dispatcher<S, G, T, PF, GT, TT, PFT>
    where
        T: Handler<TT, S, Future = HandlerFuture>,
    {
        Dispatcher {
            get: self.get,
            propfind: self.propfind,
            options: handler,
            _gt: self._gt,
            _ot: PhantomData,
            _pft: self._pft,
            _s: self._s,
        }
    }
    pub fn propfind<T, TT>(self, handler: T) -> Dispatcher<S, G, O, T, GT, OT, TT>
    where
        T: Handler<TT, S, Future = HandlerFuture>,
    {
        Dispatcher {
            get: self.get,
            propfind: handler,
            options: self.options,
            _gt: self._gt,
            _pft: PhantomData,
            _ot: self._ot,
            _s: self._s,
        }
    }
    pub fn dispatch(self) -> MethodRouter<S>
    where
        S: Clone + Send + Sync + 'static,
        G: Clone + Send + Sync + 'static + Handler<GT, S, Future = HandlerFuture>,
        O: Clone + Send + Sync + 'static + Handler<OT, S, Future = HandlerFuture>,
        PF: Clone + Send + Sync + 'static + Handler<PFT, S, Future = HandlerFuture>,
        GT: Send + Sync + 'static,
        OT: Send + Sync + 'static,
        PFT: Send + Sync + 'static,
    {
        axum::routing::any::<_, (), _>(self)
    }
}

type HandlerFuture = Pin<Box<dyn Future<Output = Response> + Send + 'static>>;

impl<S, G, O, PF, GT, OT, PFT> Handler<(), S> for Dispatcher<S, G, O, PF, GT, OT, PFT>
where
    S: Send + Sync + 'static,
    G: Clone + Send + Sync + 'static + Handler<GT, S, Future = HandlerFuture>,
    O: Clone + Send + Sync + 'static + Handler<OT, S, Future = HandlerFuture>,
    PF: Clone + Send + Sync + 'static + Handler<PFT, S, Future = HandlerFuture>,
    GT: Send + Sync + 'static,
    OT: Send + Sync + 'static,
    PFT: Send + Sync + 'static,
{
    type Future = HandlerFuture;

    fn call(self, req: Request, state: S) -> Self::Future {
        match req.method().as_str() {
            "GET" => self.get.call(req, state),
            "OPTIONS" => self.options.call(req, state),
            "PROPFIND" => self.propfind.call(req, state),
            _ => Box::pin(async move { StatusCode::METHOD_NOT_ALLOWED.into_response() }),
        }
    }
}

impl<T, S> Handler<T, S> for NonAllowedHandler {
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, _req: Request, _state: S) -> Self::Future {
        Box::pin(async move { StatusCode::METHOD_NOT_ALLOWED.into_response() })
    }
}

impl<S, G, O, PF, GT, OT, PFT> Clone for Dispatcher<S, G, O, PF, GT, OT, PFT>
where
    G: Clone,
    O: Clone,
    PF: Clone,
{
    fn clone(&self) -> Self {
        Self {
            get: self.get.clone(),
            options: self.options.clone(),
            propfind: self.propfind.clone(),
            _gt: self._gt,
            _ot: self._ot,
            _pft: self._pft,
            _s: self._s,
        }
    }
}
