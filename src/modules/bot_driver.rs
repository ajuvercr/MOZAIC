use messaging::reactor::*;
use messaging::types::*;
use errors::*;
use core_capnp::{initialize};

use cmd_capnp::{bot_return, bot_input};

use std::process::{Command, Stdio};
use std::io::BufReader;

use runtime::BrokerHandle;

use futures::{Future, Stream};

use tokio_process::{ChildStdout, CommandExt};

enum Bot {
    ToSpawn(Vec<String>),
    Spawned(mpsc::Sender<Vec<u8>>),
}

/// Reactor to handle cmd input
pub struct BotReactor {
    foreign_id: ReactorId,
    broker: BrokerHandle,
    bot: Bot,
}

impl BotReactor {
    pub fn new(broker: BrokerHandle, foreign_id: ReactorId, bot_cmd: Vec<String>) -> Self {

        Self {
            broker, foreign_id, bot: Bot::ToSpawn(bot_cmd)
        }
    }

    pub fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(bot_input::Owned, CtxHandler::new(Self::handle_return));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()>
    {
        let args = match &self.bot {
            Bot::ToSpawn(v) => v,
            _ => return Ok(())
        };

        let mut cmd = Command::new(&args[0]);
        cmd.args(&args[1..]);

        cmd.stdout(Stdio::piped());
        cmd.stdin(Stdio::piped());

        let mut bot = cmd.spawn_async()
            .expect("Couldn't spawn bot");

        let stdout = bot.stdout().take()
            .expect("child did not have a handle to stdout");

        let stdin = bot.stdin().take()
            .expect("child did not have a handle to stdin");

        let child_future = bot
            .map(|status| println!("child status was: {}", status))
            .map_err(|e| panic!("error while running child: {}", e));

        tokio::spawn(child_future);


        self.bot = Bot::Spawned(BotSink::new(stdin));

        handle.open_link(BotLink::params(handle.id().clone()))?;
        handle.open_link(ForeignLink::params(self.foreign_id.clone()))?;

        setup_async_bot_stdout(self.broker.clone(), handle.id().clone(), stdout);
        Ok(())
    }

    fn handle_return<C: Ctx>(
        &mut self,
        _: &mut ReactorHandle<C>,
        r: bot_input::Reader,
    ) -> Result<()> {

        if let Bot::Spawned(ref mut stdin) = self.bot {
            let msg = r.get_input()?;
            let mut msg = msg.to_vec();
            msg.push(b'\n');

            stdin.try_send(msg)
                .expect("Damm it");
        }

        Ok(())
    }
}

/// Link from the cmd reactor to somewhere, sending through the cmd messages
/// Also listening for messages that have to return to the command line
struct ForeignLink;

impl ForeignLink {
    pub fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);
        params.internal_handler(
            bot_return::Owned,
            CtxHandler::new(bot_return::i_to_e),
        );

        params.external_handler(
            bot_input::Owned,
            CtxHandler::new(bot_input::e_to_i)
        );

        return params;
    }
}

/// Link from the bot to the reactor, only handling incoming messages
struct BotLink;
impl BotLink {
    pub fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, Self);
        params.external_handler(bot_return::Owned, CtxHandler::new(bot_return::e_to_i), );
        return params;
    }
}

fn setup_async_bot_stdout(mut broker: BrokerHandle, id: ReactorId, stdout: ChildStdout) {
    tokio::spawn(
        tokio::io::lines(BufReader::new(stdout))
            // Convert any io::Error into a failure::Error for better flexibility
            .map_err(|e| eprintln!("{:?}", e))
            .for_each(move |input| {
                broker.send_message(
                    &id,
                    &id,
                    bot_return::Owned,
                    move |b| {
                        let mut msg: bot_return::Builder = b.init_as();
                        msg.set_message(&input.as_bytes());
                    }
                ).display();
                Ok(())
            })
    );
}

use tokio::sync::mpsc;
use std::io::Cursor;
use std::collections::VecDeque;

use tokio::io::AsyncWrite;
use tokio::prelude::*;

struct BotSink<A> {
    rx: mpsc::Receiver<Vec<u8>>,
    current: Option<Cursor<Vec<u8>>>,
    write: A,
    queue: VecDeque<Vec<u8>>,
}

impl<A> BotSink<A>
    where A: AsyncWrite + Send + 'static {

    fn new(write: A) -> mpsc::Sender<Vec<u8>> {
        let (tx, rx) = mpsc::channel(10);

        let me = Self {
            rx,
            current: None,
            write,
            queue: VecDeque::new(),
        };

        tokio::spawn(me);

        tx
    }
}

use bytes::Buf;
impl<A> Future for BotSink<A>
    where A: AsyncWrite {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        while let Ok(Async::Ready(result)) = self.rx.poll() {
            match result {
                None => return Ok(Async::Ready(())),
                Some(vec) => {
                    self.queue.push_back(vec);
                },
            }
        }

        loop {
            match self.write.poll_flush() {
                Err(_) => return Ok(Async::Ready(())),
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                _ => {}
            }

            if self.current.is_none() {
                self.current = self.queue.pop_front().map(|v| Cursor::new(v));
            }

            let current = match &mut self.current {
                None => return Ok(Async::NotReady),
                Some(ref mut c) => c,
            };

            match self.write.write_buf(current) {
                Err(_) => return Ok(Async::Ready(())),
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(_)) => {
                    if !current.has_remaining() {
                        self.current = None;
                    }
                }
            }
        }
    }
}