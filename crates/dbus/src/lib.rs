//! The blocking D-Bus calls karakulakd and the compositor make, on top of
//! zbus 6, which is async only. Each call runs zbus' async call to the end
//! with `zbus::block_on`. Between calls a zbus helper thread keeps the
//! connections working, so objects we serve (the Bluetooth pairing agent)
//! still answer.
//!
//! The API is the part of zbus 5's `blocking` module that Karakulak used.

use std::pin::Pin;

use zbus::block_on;
use zbus::export::futures_core::Stream;
use zbus::names::{BusName, InterfaceName, MemberName};

pub use zbus;
pub use zbus::{
    Error, Message, ObjectPath, OwnedFd, OwnedObjectPath, OwnedVariant, Result, Type, Variant, fdo, interface,
};

#[derive(Clone)]
pub struct Connection(zbus::Connection);

impl Connection {
    pub fn system() -> Result<Self> {
        block_on(zbus::Connection::system()).map(Self)
    }

    pub fn object_server(&self) -> ObjectServer<'_> {
        ObjectServer(self.0.object_server())
    }
}

pub struct ObjectServer<'a>(&'a zbus::ObjectServer);

impl ObjectServer<'_> {
    /// Serves `iface` at `path`; false if something is served there already.
    pub fn at<'p, P, I>(&self, path: P, iface: I) -> Result<bool>
    where
        I: zbus::object_server::Interface,
        P: TryInto<ObjectPath<'p>>,
        P::Error: Into<Error>,
    {
        block_on(self.0.at(path, iface)).map(|refused| refused.is_none())
    }
}

pub struct Proxy<'a>(zbus::Proxy<'a>);

impl<'a> Proxy<'a> {
    pub fn new<D, P, I>(conn: &Connection, destination: D, path: P, interface: I) -> Result<Self>
    where
        D: TryInto<BusName<'a>>,
        P: TryInto<ObjectPath<'a>>,
        I: TryInto<InterfaceName<'a>>,
        D::Error: Into<Error>,
        P::Error: Into<Error>,
        I::Error: Into<Error>,
    {
        block_on(zbus::Proxy::new(&conn.0, destination, path, interface)).map(Self)
    }

    pub fn get_property<T>(&self, name: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned + Type,
    {
        block_on(self.0.get_property(name))
    }

    pub fn set_property<T>(&self, name: &str, value: T) -> fdo::Result<()>
    where
        T: serde::Serialize + Type,
    {
        block_on(self.0.set_property(name, value))
    }

    pub fn call_method<'m, M, B>(&self, method: M, body: &B) -> Result<Message>
    where
        M: TryInto<MemberName<'m>>,
        M::Error: Into<Error>,
        B: serde::Serialize + zbus::DynamicType,
    {
        block_on(self.0.call_method(method, body))
    }

    pub fn call<'m, M, B, R>(&self, method: M, body: &B) -> Result<R>
    where
        M: TryInto<MemberName<'m>>,
        M::Error: Into<Error>,
        B: serde::Serialize + zbus::DynamicType,
        R: for<'d> zbus::wire::DynamicDeserialize<'d>,
    {
        block_on(self.0.call(method, body))
    }

    /// Every `name` signal, as it arrives.
    pub fn receive_signal<'m, M>(&self, name: M) -> Result<Iter<zbus::proxy::SignalStream<'m>>>
    where
        M: TryInto<MemberName<'m>>,
        M::Error: Into<Error>,
    {
        block_on(self.0.receive_signal(name)).map(Iter)
    }

    /// The property's value first, then every change.
    pub fn receive_property_changed<T>(&self, name: &'a str) -> Iter<zbus::proxy::PropertyStream<'a, T>>
    where
        T: Unpin,
    {
        Iter(block_on(self.0.receive_property_changed(name)))
    }
}

/// A stream read one item at a time, waiting for each.
pub struct Iter<S>(S);

impl<S: Stream + Unpin> Iterator for Iter<S> {
    type Item = S::Item;

    fn next(&mut self) -> Option<S::Item> {
        block_on(std::future::poll_fn(|cx| Pin::new(&mut self.0).poll_next(cx)))
    }
}
