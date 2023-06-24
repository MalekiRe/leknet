use crate::{
    connect_to_server, start_server, ClientMessage, ClientMessageMap, LekClient, LeknetClient,
    LeknetServer, Message, ServerMessage, ServerMessageMap, TypeName,
};
use bevy::MinimalPlugins;
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::{Commands, Component, ReflectComponent, ResMut, World};
use bevy_quinnet::client::certificate::CertificateVerificationMode;
use bevy_quinnet::client::connection::ConnectionConfiguration;
use bevy_quinnet::client::{Client, QuinnetClientPlugin};
use bevy_quinnet::server::certificate::CertificateRetrievalMode;
use bevy_quinnet::server::{QuinnetServerPlugin, Server, ServerConfiguration};
use bevy_quinnet::shared::channel::{ChannelId, ChannelType};
use bevy_quinnet::shared::ClientId;
use bevy_reflect::{Reflect, TypeUuidDynamic};
use port_scanner::request_open_port;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::net::{SocketAddr, SocketAddrV4};

#[derive(Debug, Serialize, Deserialize)]
pub enum TestMessage {
    Hi,
    Bye,
}

impl TypeName for TestMessage {
    fn get_type_name() -> String {
        "leknet::test::TestMessage".to_string()
    }
}

impl ClientMessage for TestMessage {
    fn client(self, world: &mut World) {
        println!("client rec: {:?}", self)
    }

    fn _client(world: &mut World, msg_bytes: &[u8]) {
        bincode::deserialize::<Self>(msg_bytes)
            .unwrap()
            .client(world)
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::OrderedReliable
    }

    fn plugin(app: &mut App) {
        //add stuff you wanna setup this app with
    }
}

impl ServerMessage for TestMessage {
    fn server(self, world: &mut World, client_id: ClientId) {
        println!("server rec: {:?}", self)
    }

    fn _server(world: &mut World, msg_bytes: &[u8], client_id: ClientId) {
        bincode::deserialize::<Self>(msg_bytes)
            .unwrap()
            .server(world, client_id)
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::OrderedReliable
    }

    fn plugin(app: &mut App) {
        //add stuff you wanna setup this app with
    }
}

fn start() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app
}

// tests are commented out because they rely on some minimal plugins being installed, but you don't need those plugins in order to work
// #[test]
// fn server() {
//     let mut app = start();
//     TestMessage::add_plugin_server(&mut app);
//     app.add_plugin(LeknetServer);
//     app.add_plugin(QuinnetServerPlugin::default());
//     app.add_startup_system(start_server);
//     app.run();
// }
//
// #[test]
// fn client() {
//     let mut app = start();
//     TestMessage::add_plugin_client(&mut app);
//     app.add_plugin(QuinnetClientPlugin::default());
//     app.add_plugin(LeknetClient);
//     app.add_startup_system(connect_to_server);
//     app.add_system(my_system);
//     app.run();
// }

static mut THING: bool = false;

fn my_system(mut client: ResMut<Client>) {
    if let Some(connection_mut) = client.get_connection_mut() {
        if unsafe { THING == false } {
            unsafe {
                THING = true;
            }
            connection_mut.send_lek_msg(TestMessage::Hi).unwrap();
        }
    }
}

#[test]
pub fn test_ports() {}
