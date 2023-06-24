#[cfg(test)]
mod test;

use bevy_app::{App, Plugin};
use bevy_ecs::entity::Entity;
use bevy_ecs::event::{EventReader, EventWriter};
use bevy_ecs::prelude::{Component, ResMut, Resource};
use bevy_ecs::system::SystemState;
use bevy_ecs::world::World;
use bevy_quinnet::client::certificate::CertificateVerificationMode;
use bevy_quinnet::client::connection::{Connection, ConnectionConfiguration};
use bevy_quinnet::client::Client;
use bevy_quinnet::server::certificate::CertificateRetrievalMode;
use bevy_quinnet::server::{ClientConnection, Endpoint, Server, ServerConfiguration};
use bevy_quinnet::shared::channel::{ChannelId, ChannelType};
use bevy_quinnet::shared::{ClientId, QuinnetError};
use bevy_reflect::Reflect;
use bimap::BiHashMap;
use port_scanner::request_open_port;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::net::{SocketAddr, SocketAddrV4};
use std::ops::{Deref, DerefMut};

#[derive(Resource)]
pub struct ServerMessageMap(
    pub HashMap<String, Box<(dyn Fn(&mut World, &[u8], ClientId) + Sync + Send)>>,
);
#[derive(Resource)]
pub struct ClientMessageMap(pub HashMap<String, Box<(dyn Fn(&mut World, &[u8]) + Sync + Send)>>);
#[derive(Resource)]
pub struct EntityMap(pub BiHashMap<ClientEntity, ServerEntity>);

#[derive(Component)]
pub struct Networked;

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct ClientEntity(pub Entity);
#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct ServerEntity(pub Entity);

impl Deref for EntityMap {
    type Target = BiHashMap<ClientEntity, ServerEntity>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for EntityMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub trait LekServer {
    fn send_lek_msg(
        &mut self,
        client_id: ClientId,
        message: impl ClientMessage,
    ) -> Result<(), QuinnetError>;
}
impl LekServer for Endpoint {
    fn send_lek_msg(
        &mut self,
        client_id: ClientId,
        message: impl ClientMessage,
    ) -> Result<(), QuinnetError> {
        self.send_message_on(client_id, message.channel_id(), message.to_message())
    }
}
pub trait LekClient {
    fn send_lek_msg(&mut self, message: impl ServerMessage) -> Result<(), QuinnetError>;
}
impl LekClient for Connection {
    fn send_lek_msg(&mut self, message: impl ServerMessage) -> Result<(), QuinnetError> {
        self.send_message_on(message.channel_id(), message.to_message())
    }
}

pub trait TypeName {
    fn get_type_name() -> String;
}

pub trait ClientMessage: Any + serde::Serialize + TypeName {
    fn client(self, world: &mut World);
    /// bincode::deserialize::<Self>(msg_bytes).unwrap().client(world)
    fn _client(world: &mut World, msg_bytes: &[u8]);
    fn channel_id(&self) -> ChannelId {
        match self.channel_type() {
            ChannelType::OrderedReliable => ChannelId::OrderedReliable(1),
            ChannelType::UnorderedReliable => ChannelId::UnorderedReliable,
            ChannelType::Unreliable => ChannelId::Unreliable,
        }
    }
    fn channel_type(&self) -> ChannelType;
    fn to_message(&self) -> Message {
        Message {
            name: Self::get_type_name(),
            data: bincode::serialize(self).unwrap(),
        }
    }
    fn client_system(mut client_msg_map: ResMut<ClientMessageMap>) {
        client_msg_map
            .0
            .insert(Self::get_type_name(), Box::new(Self::_client));
    }
    fn add_plugin_client(app: &mut App) {
        app.add_startup_system(Self::client_system);
        Self::plugin(app);
    }
    #[deprecated]
    fn plugin(app: &mut App);
}

pub trait ServerMessage: Any + serde::Serialize + TypeName {
    fn server(self, world: &mut World, client_id: ClientId);
    fn _server(world: &mut World, msg_bytes: &[u8], client_id: ClientId);
    fn channel_id(&self) -> ChannelId {
        match self.channel_type() {
            ChannelType::OrderedReliable => ChannelId::OrderedReliable(1),
            ChannelType::UnorderedReliable => ChannelId::UnorderedReliable,
            ChannelType::Unreliable => ChannelId::Unreliable,
        }
    }
    fn channel_type(&self) -> ChannelType;
    fn to_message(&self) -> Message {
        Message {
            name: Self::get_type_name(),
            data: bincode::serialize(self).unwrap(),
        }
    }
    fn server_system(mut server_msg_map: ResMut<ServerMessageMap>) {
        server_msg_map
            .0
            .insert(Self::get_type_name(), Box::new(Self::_server));
    }
    fn add_plugin_server(app: &mut App) {
        app.add_startup_system(Self::server_system);
        Self::plugin(app);
    }
    #[deprecated]
    fn plugin(app: &mut App);
}

pub struct LeknetServer;
pub struct LeknetClient;

impl Plugin for LeknetServer {
    fn build(&self, app: &mut App) {
        app.insert_resource(EntityMap(BiHashMap::new()));
        app.insert_resource(ServerMessageMap(HashMap::new()));
        app.add_system(server_msg);
        app.add_event::<ServerMsg>();
    }
}

impl Plugin for LeknetClient {
    fn build(&self, app: &mut App) {
        app.insert_resource(EntityMap(BiHashMap::new()));
        app.insert_resource(ClientMessageMap(HashMap::new()));
        app.add_system(client_msg);
        app.add_event::<ClientMsg>();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    name: String,
    data: Vec<u8>,
}

/// message from the client to the server
#[derive(Clone)]
struct ServerMsg(String, Vec<u8>, ClientId);
/// message from the server to the client
#[derive(Clone)]
struct ClientMsg(String, Vec<u8>);

fn server_msg(world: &mut World) {
    let mut system_state: SystemState<ResMut<ServerMessageMap>> =
        SystemState::new(world);
    if system_state.get_mut(world).0.keys().len() == 0 {
        return;
    }
    let mut system_state: SystemState<ResMut<Server>> = SystemState::new(world);

    let mut server: ResMut<Server> = system_state.get_mut(world);

    let mut messages = Vec::new();
    if let Some(endpoint) = server.get_endpoint_mut() {
        for client_id in endpoint.clients() {
            while let Some(message) =
                endpoint.try_receive_message_from::<Message>(client_id.clone())
            {
                messages.push(ServerMsg(message.name, message.data, client_id.clone()));
            }
        }
    }

    for msg in messages {
        match msg {
            ServerMsg(name, data, client_id) => {
                let mut system_state: SystemState<ResMut<ServerMessageMap>> =
                    SystemState::new(world);
                let mut func = None;
                {
                    func.replace(system_state.get_mut(world).0.remove(&name).unwrap());
                }
                let func = func.unwrap();
                func(world, data.as_slice(), client_id);
                system_state.get_mut(world).0.insert(name, func);
            }
        }
    }
}

fn client_msg(world: &mut World) {
    let mut system_state: SystemState<ResMut<ClientMessageMap>> =
        SystemState::new(world);
    if system_state.get_mut(world).0.keys().len() == 0 {
        return;
    }

    let mut system_state: SystemState<ResMut<Client>> = SystemState::new(world);

    let mut client = system_state.get_mut(world);

    let mut messages = Vec::new();

    for (_id, connection) in client.connections_mut() {
        while let Some(message) = connection.try_receive_message::<Message>() {
            messages.push(ClientMsg(message.name, message.data))
        }
    }

    for msg in messages {
        match msg {
            ClientMsg(name, data) => {
                let mut system_state: SystemState<ResMut<ClientMessageMap>> =
                    SystemState::new(world);
                let mut func = None;
                {
                    func.replace(system_state
                        .get_mut(world)
                        .0
                        .remove(&name)
                        .unwrap());
                }
                let func = func.unwrap();
                func(world, data.as_slice());
                system_state.get_mut(world).0.insert(name, func);
            }
        }
    }
}

#[allow(dead_code)]
pub fn start_server(mut server: ResMut<Server>) {
    server
        .start_endpoint(
            ServerConfiguration::from_addr(server_addr()),
            CertificateRetrievalMode::GenerateSelfSigned {
                server_hostname: "myserver".to_string(),
            },
        )
        .unwrap();
}

#[allow(dead_code)]
pub fn connect_to_server(mut client: ResMut<Client>) {
    let port = request_open_port().expect("Unable to find an open port");
    client
        .open_connection(
            ConnectionConfiguration::from_addrs(
                server_addr(),
                SocketAddr::V4(SocketAddrV4::new("0.0.0.0".parse().unwrap(), port)),
            ),
            CertificateVerificationMode::SkipVerification,
        )
        .unwrap();
}

pub const SERVER_ADDR: &'static str = "127.0.0.1:5000";

pub fn server_addr() -> SocketAddr {
    SERVER_ADDR.parse().unwrap()
}
