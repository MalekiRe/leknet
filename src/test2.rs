use std::net::{SocketAddr, UdpSocket};
use std::time::SystemTime;
use bevy::MinimalPlugins;
use bevy::reflect::TypePath;
use bevy_app::{App, Update};
use bevy_renet::renet::{ConnectionConfig, DefaultChannel, RenetClient, RenetServer};
use bevy_renet::renet::transport::{ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication, ServerConfig};
use bevy_renet::{RenetClientPlugin, RenetServerPlugin};
use bevy_renet::transport::{NetcodeClientPlugin, NetcodeServerPlugin};
use serde::{Deserialize, Serialize};
use crate::{Channel, ClientChannelReader, ClientChannelWriter, LeknetClientPlugin, LeknetServerPlugin, ServerChannelReader, ServerChannelWriter};
use crate::RegisterChannel;


#[test]
fn client() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((LeknetClientPlugin, RenetClientPlugin, NetcodeClientPlugin));
    app.register_channel::<ServerMessage>(5 * 1024 * 1024);
    app.register_channel::<ClientMessage>(5 * 1024 * 1024);
    app.add_systems(Update, bar);
    app.add_systems(Update, baz);
    let public_addr: SocketAddr = "127.0.0.1:5001".parse().unwrap();
    let socket = UdpSocket::bind(public_addr).unwrap();
    let client_authentication = ClientAuthentication::Unsecure {
        protocol_id: 0,
        client_id: 0,
        server_addr: "127.0.0.1:5000".parse().unwrap(),
        user_data: None,
    };
    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let transport = NetcodeClientTransport::new(current_time, client_authentication, socket).unwrap();
    app.insert_resource(transport);
    app.run();
}

#[test]
fn server() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((LeknetServerPlugin, RenetServerPlugin, NetcodeServerPlugin));
    app.register_channel::<ServerMessage>(5 * 1024 * 1024);
    app.register_channel::<ClientMessage>(5 * 1024 * 1024);
    app.add_systems(Update, foo);
    app.add_systems(Update, faz);
    let server_addr = "127.0.0.1:5000".parse().unwrap();
    let socket = UdpSocket::bind(server_addr).unwrap();
    let server_config = ServerConfig {
        max_clients: 64,
        protocol_id: 0,
        public_addr: server_addr,
        authentication: ServerAuthentication::Unsecure,
    };
    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let transport = NetcodeServerTransport::new(current_time, server_config, socket).unwrap();
    app.insert_resource(transport);
    app.run();
}

fn foo(mut scw: ServerChannelWriter<ServerMessage>) {
    for client in scw.clients_id() {
        scw.send(ServerMessage::HiThere, DefaultChannel::ReliableOrdered, client);
        scw.send(ServerMessage::Sup(client as i32), DefaultChannel::Unreliable, client);
    }
}
fn faz(mut scr: ServerChannelReader<ClientMessage>) {
    for msg in scr.read() {
        eprintln!("received: {:#?}", msg);
    }
}
fn baz(mut ccw: ClientChannelWriter<ClientMessage>) {
    ccw.send(ClientMessage::Hello, DefaultChannel::ReliableOrdered);
    ccw.send(ClientMessage::Yo(1), DefaultChannel::Unreliable);
}
fn bar(mut ccr: ClientChannelReader<ServerMessage>) {
    for msg in ccr.read() {
        eprintln!("received: {:#?}", msg);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TypePath)]
pub enum ServerMessage {
    HiThere,
    Sup(i32)
}

#[derive(Debug, Clone, Serialize, Deserialize, TypePath)]
pub enum ClientMessage {
    Hello,
    Yo(u32),
}

impl Channel for ServerMessage {}
impl Channel for ClientMessage {}