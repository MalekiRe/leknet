#[cfg(test)]
mod test2;

use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex};
use bevy_ecs::prelude::{Res, ResMut};
use bevy_ecs::system::SystemParam;
use bevy_reflect::{TypePath};
use serde::{Deserialize, Serialize};
use bevy::prelude::Resource;
use bevy_app::{App, Plugin};
use bevy_renet::renet::{ChannelConfig, ConnectionConfig, DefaultChannel, RenetClient, RenetServer, SendType};


pub struct LeknetClientPlugin;
pub struct LeknetServerPlugin;

#[derive(SystemParam)]
pub struct ClientChannelWriter<'w, T: Channel> {
    client: ResMut<'w, RenetClient>,
    channel_map: Res<'w, ChannelMap>,
    phantom: PhantomData<T>,
}
impl <'a, T: Channel> ClientChannelWriter<'a, T> {
    pub fn send(&mut self, data: T, send_type: DefaultChannel) {
        let ids = self.channel_map.get(T::short_type_path());
        let channel_id = match send_type {
            DefaultChannel::Unreliable => ids[0],
            DefaultChannel::ReliableOrdered => ids[1],
            DefaultChannel::ReliableUnordered => ids[2],
        };
        self.client.send_message(channel_id, bincode::serialize(&data).unwrap());
    }
}

#[derive(SystemParam)]
pub struct ClientChannelReader<'w, T: Channel> {
    rec_map: ResMut<'w, ReceiverMap>,
    phantom_data: PhantomData<T>,
}
impl<'w, T: Channel> ClientChannelReader<'w, T> {
    pub fn read(&mut self) -> Vec<T> {
        let a = self.rec_map.rec_map.get(T::short_type_path()).unwrap();
        let b = a.lock().unwrap();
        let mut messages = vec![];
        for msg in b.try_iter() {
            messages.push(bincode::deserialize(&msg).unwrap());
        }
        messages
    }
}

#[derive(SystemParam)]
pub struct ServerChannelWriter<'w, T: Channel> {
    server: ResMut<'w, RenetServer>,
    channel_map: Res<'w, ChannelMap>,
    phantom: PhantomData<T>,
}
impl <'a, T: Channel> ServerChannelWriter<'a, T> {
    pub fn send(&mut self, data: T, send_type: DefaultChannel, client_id: u64) {
        let ids = self.channel_map.get(T::short_type_path());
        let channel_id = match send_type {
            DefaultChannel::Unreliable => ids[0],
            DefaultChannel::ReliableUnordered => ids[1],
            DefaultChannel::ReliableOrdered => ids[2],
        };
        self.server.send_message(client_id, channel_id, bincode::serialize(&data).unwrap());
    }
    pub fn clients_id(&self) -> Vec<u64> {
        self.server.clients_id()
    }
}

#[derive(SystemParam)]
pub struct ServerChannelReader<'w, T: Channel> {
    rec_map: ResMut<'w, ReceiverMap>,
    phantom_data: PhantomData<T>,
}
impl<'w, T: Channel> ServerChannelReader<'w, T> {
    pub fn read(&mut self) -> Vec<T> {
        let a = self.rec_map.rec_map.get(T::short_type_path()).unwrap();
        let b = a.lock().unwrap();
        let mut messages = vec![];
        for msg in b.try_iter() {
            messages.push(bincode::deserialize(&msg).unwrap());
        }
        messages
    }
}


pub trait Channel: TypePath + std::fmt::Debug + Send + Sync + 'static + Serialize + for<'a> Deserialize<'a> {}

#[derive(Resource, Default)]
pub struct ChannelMap{
    pub channel_map: HashMap<&'static str, [u8; 3]>,
}
#[derive(Resource, Default)]
pub struct SenderMap {
    pub send_map: HashMap<&'static str, Mutex<Sender<Vec<u8>>>>
}
#[derive(Resource, Default)]
pub struct ReceiverMap {
    pub rec_map: HashMap<&'static str, Mutex<Receiver<Vec<u8>>>>
}
impl ChannelMap {
    pub fn add_channels(&mut self, name: &'static str) {
        if self.channel_map.len() >= 255 {
            panic!("too many channels added, max of 256");
        }
        let start = self.channel_map.len() as u8;
        self.channel_map.insert(name, [start, start+1, start+2]);
    }
    pub fn get(&self, name: &'static str) -> [u8; 3] {
        *self.channel_map.get(name).unwrap()
    }
}

impl Plugin for LeknetClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ChannelMap::default());
        app.insert_resource(SenderMap::default());
        app.insert_resource(ReceiverMap::default());
        app.insert_resource(RenetClient::new(ConnectionConfig::default()));
        app.add_systems(bevy_app::Update, |channel_map: Res<ChannelMap>, mut client: ResMut<RenetClient>, send_map: ResMut<SenderMap>| {
            let c = &channel_map.channel_map;
            for (key, i) in c.iter() {
                for i in i {
                    if let Some(msg) = client.receive_message(*i) {
                        send_map.send_map.get(key).unwrap().lock().unwrap().send(msg.to_vec()).unwrap();
                    }
                }
            }
        });
    }
}

impl Plugin for LeknetServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ChannelMap::default());
        app.insert_resource(SenderMap::default());
        app.insert_resource(ReceiverMap::default());
        app.insert_resource(RenetServer::new(ConnectionConfig::default()));
        app.add_systems(bevy_app::Update, |channel_map: Res<ChannelMap>, mut server: ResMut<RenetServer>, send_map: ResMut<SenderMap>| {
            let c = &channel_map.channel_map;
            for (key, i) in c.iter() {
                for i in i {
                    for client in server.clients_id() {
                        if let Some(msg) = server.receive_message(client, *i) {
                            send_map.send_map.get(key).unwrap().lock().unwrap().send(msg.to_vec()).unwrap();
                        }
                    }
                }
            }
        });
    }
}

pub trait RegisterChannel {
    fn register_channel<T: Channel>(&mut self, max_memory_usage_bytes: usize);
}

impl RegisterChannel for App {
    fn register_channel<T: Channel>(&mut self, max_memory_usage_bytes: usize) {
        let (tx, rx) = channel();
        self.world.resource_mut::<ChannelMap>().add_channels(T::short_type_path());
        self.world.resource_mut::<SenderMap>().send_map.insert(T::short_type_path(), Mutex::new(tx));
        self.world.resource_mut::<ReceiverMap>().rec_map.insert(T::short_type_path(), Mutex::new(rx));
        let len = self.world.resource_mut::<ChannelMap>().channel_map.len() as u8;
        let mut channels_config = vec![];
        let mut i = 0;
        while i < len*3 {
            channels_config.push(ChannelConfig {
                channel_id: i,
                max_memory_usage_bytes,
                send_type: SendType::Unreliable,
            });
            channels_config.push(ChannelConfig {
                channel_id: i+1,
                max_memory_usage_bytes,
                send_type: SendType::ReliableUnordered {
                    resend_time: Default::default(),
                },
            });
            channels_config.push(ChannelConfig {
                channel_id: i+2,
                max_memory_usage_bytes,
                send_type: SendType::ReliableOrdered {
                    resend_time: Default::default(),
                },
            });
            i += 3;
        }
        let connection_config = ConnectionConfig {
            available_bytes_per_tick: 60_000,
            server_channels_config: channels_config.clone(),
            client_channels_config: channels_config,
        };
        if self.world.get_resource::<RenetClient>().is_some() {
            self.insert_resource(RenetClient::new(connection_config))
        } else {
            self.insert_resource(RenetServer::new(connection_config))
        };
    }
}