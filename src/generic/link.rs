use super::*;
use std::hash::Hash;

// ANCHOR Link
pub struct Link<S, K, M> {
    state: S,

    internal_handlers: HashMap<K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>>,
    external_handlers: HashMap<K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>>,

    handles: LinkHandle<K, M>,
}

impl<S, K, M> Link<S, K, M> {
    pub fn new(handles: LinkHandle<K, M>, params: LinkParams<S, K, M>) -> Self {
        Self {
            handles,
            state: params.state,
            internal_handlers: params.internal_handlers,
            external_handlers: params.external_handlers,
        }
    }
}

#[derive(Clone)]
pub struct LinkHandle<K, M> {
    source: Sender<K, M>,
    target: Sender<K, M>,
    target_id: ReactorID,
}

impl LinkHandle<any::TypeId, Message> {
    pub fn send_message<T: 'static>(&mut self, msg: T) {
        println!("Sending message 2");
        let id = any::TypeId::of::<T>();
        let msg = Message::from(msg);
        self.target
            .unbounded_send(Operation::ExternalMessage(self.target_id.clone(), id, msg))
            .expect("Crashed");
    }
}

impl<'a, 'b, S, K, M> Handler<(), ReactorHandle<'b, K, M>, LinkOperation<'a, K, M>>
    for Link<S, K, M>
where
    K: Hash + Eq,
{
    fn handle(
        &mut self,
        _: &mut (),
        _handle: &mut ReactorHandle<'b, K, M>,
        m: &mut LinkOperation<K, M>,
    ) {
        match m {
            LinkOperation::InternalMessage(id, message) => {
                if let Some(h) = self.internal_handlers.get_mut(id) {
                    h.handle(&mut self.state, &mut self.handles, message);
                }
            }
            LinkOperation::ExternalMessage(id, message) => {
                if let Some(h) = self.external_handlers.get_mut(id) {
                    h.handle(&mut self.state, &mut self.handles, message);
                }
            }
        };
    }
}

// ANCHOR Params
pub struct LinkParams<S, K, M> {
    state: S,
    internal_handlers: HashMap<K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>>,
    external_handlers: HashMap<K, Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>>,
}

impl<S, K, M> LinkParams<S, K, M>
where
    K: Eq + Hash,
{
    pub fn new(state: S) -> Self {
        Self {
            state,
            internal_handlers: HashMap::new(),
            external_handlers: HashMap::new(),
        }
    }

    pub fn internal_handler(
        &mut self,
        id: K,
        handler: Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>,
    ) {
        self.internal_handlers.insert(id, handler);
    }

    pub fn external_handler(
        &mut self,
        id: K,
        handler: Box<dyn Handler<S, LinkHandle<K, M>, M> + Send>,
    ) {
        self.external_handlers.insert(id, handler);
    }
}

impl<S, K, M> Into<LinkSpawner<K, M>> for LinkParams<S, K, M>
where
    S: 'static + Send,
    M: 'static + Send,
    K: 'static + Eq + Hash + Send,
{
    fn into(self) -> LinkSpawner<K, M> {
        Box::new(move |(source, target, target_id)| {
            let handles = LinkHandle {
                source,
                target,
                target_id,
            };

            Box::new(Link::new(handles, self))
        })
    }
}