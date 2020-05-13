extern crate async_std;
extern crate futures;
extern crate mozaic;

#[macro_use]
extern crate serde_json;

extern crate tracing;
extern crate tracing_subscriber;

use tracing_subscriber::{EnvFilter, FmtSubscriber};

use serde_json::Value;
use std::time;

use mozaic::modules::client_controller::spawner::SpawnerBuilder;
use mozaic::modules::game;
use mozaic::modules::types::*;

use futures::executor::ThreadPool;
use futures::future::FutureExt;

#[derive(Clone)]
struct Echo {
    clients: Vec<PlayerId>,
}

impl game::Controller for Echo {
    fn on_connect(&mut self, player: PlayerId) -> Vec<HostMsg> {
        let mut out = Vec::new();
        out.push(HostMsg::Data(
            Data {
                value: format!("{} connected\n", player).into_bytes(),
            },
            None,
        ));
        out
    }

    fn on_disconnect(&mut self, player: PlayerId) -> Vec<HostMsg> {
        println!("{} disconnected", player);
        let mut out = Vec::new();
        out.push(HostMsg::Data(
            Data {
                value: format!("{} disconnected\n", player).into_bytes(),
            },
            None,
        ));
        out
    }

    fn on_step(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg> {
        let mut sub = Vec::new();
        for PlayerMsg { id, data } in turns {
            let msg = data
                .map(|x| String::from_utf8(x.value).unwrap())
                .unwrap_or_else(|| "TIMEOUT".to_string());
            if "stop".eq_ignore_ascii_case(&msg) {
                sub.push(HostMsg::kick(id));
                self.clients = self.clients.iter().cloned().filter(|&x| x != id).collect();
            }

            sub.push(HostMsg::Data(
                Data {
                    value: format!("{}: {}\n", id, msg).into_bytes(),
                },
                None,
            ));
        }

        sub
    }

    fn get_state(&mut self) -> Value {
        json!({
            "Some": "players"
        })
    }

    fn is_done(&mut self) -> Option<Value> {
        if self.clients.is_empty() {
            let value = json!({"testing": 123});
            Some(value)
        } else {
            None
        }
    }
}

use mozaic::graph;
use mozaic::modules::net::{TcpEndpoint, UdpEndpoint};
use mozaic::modules::types::Uuid;

use std::collections::VecDeque;

#[async_std::main]
async fn main() -> std::io::Result<()> {
    let fut = graph::set_default();

    let sub = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(sub).unwrap();
    {
        let pool = ThreadPool::builder().create().unwrap();
        pool.spawn_ok(fut.map(|_| ()));

        let (gmb, handle) = game::Manager::builder(pool.clone());
        let ep_tcp = TcpEndpoint::new("127.0.0.1:6666".parse().unwrap(), pool.clone());
        let ep_udp = UdpEndpoint::new("127.0.0.1:6667".parse().unwrap());

        let gmb = gmb.add_endpoint(ep_tcp, "TCP endpoint");
        let gmb = gmb.add_endpoint(ep_udp, "UDP endpoint");
        let gm = gmb.build("game.ini", pool.clone()).await.unwrap();

        let mut games = VecDeque::new();

        let game_builder = {
            let players = vec![10, 11];
            let game = Echo {
                clients: players.clone(),
            };
            let uuid = Uuid::from_u128(12);
            println!("UUID {}", uuid);

            game::Builder::new(players.clone(), game)
                .with_free_client(uuid, SpawnerBuilder::new(false))
        };
        async_std::task::sleep(std::time::Duration::from_millis(100)).await;

        games.push_back(gm.start_game(game_builder.clone()).await.unwrap());
        println!("{:?}", gm.get_state(*games.back().unwrap()).await);

        loop {
            async_std::task::sleep(std::time::Duration::from_millis(3000)).await;

            match gm.get_state(*games.back().unwrap()).await {
                Some(Ok(v)) => println!("{:?}", v),
                Some(Err(e)) => {
                    println!("{:?}", e);
                    break;
                }
                None => {}
            }
        }

        handle.await;
    }

    std::thread::sleep(time::Duration::from_millis(100));

    Ok(())
}
