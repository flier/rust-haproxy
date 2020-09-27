use std::collections::HashMap;

use tokio::{
    net::TcpListener,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::spop::{Message, StreamId};
use crate::{Acker, Connection, Messages};

#[derive(Debug)]
pub struct Agent {
    listener: TcpListener,
    engines: HashMap<String, Engine>,
    sender: UnboundedSender<(Acker, Message)>,
    receiver: UnboundedReceiver<(Acker, Message)>,
}

#[derive(Debug)]
pub struct Engine {
    connections: HashMap<StreamId, Connection>,
}

impl Agent {
    pub fn messages(self) -> Messages {
        Messages(self.receiver)
    }
}
