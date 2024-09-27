use bytes::BufMut;

use crate::{
    action,
    data::BufMutExt as _,
    frame::{self, agent, haproxy, kv, Frame, Metadata, Type},
    Action,
};

pub trait BufMutExt {
    fn put_frame(&mut self, frame: Frame);
}

impl<T> BufMutExt for T
where
    T: BufMut,
{
    fn put_frame(&mut self, f: Frame) {
        frame(self, f)
    }
}

/// Put a frame into the buffer.
pub fn frame<B: BufMut>(mut buf: B, frame: Frame) {
    match frame {
        Frame::Unset => {
            buf.put_u8(Type::UNSET);
            metadata(&mut buf, Metadata::default());
        }

        Frame::HaproxyHello(hello) => {
            buf.put_u8(Type::HAPROXY_HELLO);
            metadata(&mut buf, Metadata::default());
            haproxy_hello(&mut buf, hello);
        }
        Frame::AgentHello(hello) => {
            buf.put_u8(Type::AGENT_HELLO);
            metadata(&mut buf, Metadata::default());
            agent_hello(&mut buf, hello);
        }

        Frame::HaproxyDisconnect(d) => {
            buf.put_u8(Type::HAPROXY_DISCON);
            metadata(&mut buf, Metadata::default());
            disconnect(&mut buf, d);
        }
        Frame::AgentDisconnect(d) => {
            buf.put_u8(Type::AGENT_DISCON);
            metadata(&mut buf, Metadata::default());
            disconnect(&mut buf, d);
        }

        Frame::HaproxyNotify(notify) => {
            buf.put_u8(Type::HAPROXY_NOTIFY);
            metadata(&mut buf, notify.metadata());
            haproxy_notify(&mut buf, notify);
        }
        Frame::AgentAck(ack) => {
            buf.put_u8(Type::AGENT_ACK);
            metadata(&mut buf, ack.metadata());
            agent_ack(&mut buf, ack);
        }
    }
}

pub fn metadata<B: BufMut>(mut buf: B, metadata: Metadata) {
    buf.put_u32(metadata.flags.bits().to_be());
    buf.put_varint(metadata.stream_id);
    buf.put_varint(metadata.frame_id);
}

fn haproxy_hello<B: BufMut>(mut buf: B, hello: haproxy::Hello) {
    buf.put_kv(kv::supported_versions(&hello.supported_versions));
    buf.put_kv(kv::max_frame_size(hello.max_frame_size));
    buf.put_kv(kv::capabilities(&hello.capabilities));
    if let Some(healthcheck) = hello.healthcheck {
        buf.put_kv(kv::healthcheck(healthcheck));
    }
    if let Some(ref id) = hello.engine_id {
        buf.put_kv(kv::engine_id(id));
    }
}

fn agent_hello<B: BufMut>(mut buf: B, hello: agent::Hello) {
    buf.put_kv(kv::version(hello.version));
    buf.put_kv(kv::max_frame_size(hello.max_frame_size));
    buf.put_kv(kv::capabilities(&hello.capabilities));
}

fn disconnect<B: BufMut>(mut buf: B, disconnect: frame::Disconnect) {
    buf.put_kv(kv::status_code(disconnect.status_code));
    buf.put_kv(kv::message(&disconnect.message));
}

fn haproxy_notify<B: BufMut>(mut buf: B, notify: haproxy::Notify) {
    for message in notify.messages {
        buf.put_string(message.name);
        buf.put_u8(message.args.len() as u8);
        buf.put_kvlist(message.args);
    }
}

fn agent_ack<B: BufMut>(mut buf: B, ack: agent::Ack) {
    for act in ack.actions {
        action(&mut buf, act);
    }
}

pub fn action<B: BufMut>(mut buf: B, action: Action) {
    match action {
        Action::SetVar { scope, name, value } => {
            buf.put_slice(&[action::Type::SetVar as u8, 3, scope as u8]);
            buf.put_string(name);
            buf.put_typed(value);
        }
        Action::UnsetVar { scope, name } => {
            buf.put_slice(&[action::Type::UnsetVar as u8, 2, scope as u8]);
            buf.put_string(name);
        }
    }
}
