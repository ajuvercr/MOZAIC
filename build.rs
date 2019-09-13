extern crate capnpc;

use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::env;

fn main() -> std::io::Result<()> {
    capnpc::CompilerCommand::new()
        .src_prefix("./schema")
        .file("./schema/core.capnp")
        .file("./schema/network.capnp")
        .file("./schema/chat.capnp")
        .file("./schema/planetwars.capnp")
        .file("./schema/client_events.capnp")
        .file("./schema/match_control.capnp")
        .file("./schema/match_events.capnp")
        .file("./schema/server_control.capnp")
        .file("./schema/my.capnp")
        .file("./schema/mozaic/cmd.capnp")
        .file("./schema/mozaic/logging.capnp")
        .file("./schema/mozaic/client.capnp")
        .run().expect("schema compiler command");

    let out_dir = env::var("OUT_DIR").unwrap();

    let lib_file = env::var("CARGO_MANIFEST_DIR").unwrap() + "/src/lib.rs";
    let _back_lib_file = env::var("CARGO_MANIFEST_DIR").unwrap() + "/src/back_lib.rs";

    let mut contents = String::new();
    {
        // if Path::new(&back_lib_file).exists() {
        //     let mut file = OpenOptions::new()
        //                 .read(true)
        //                 .open(&back_lib_file)?;
        //     file.read_to_string(&mut contents)?;
        // } else {
            let mut file = OpenOptions::new()
                        .read(true)
                        .open(&lib_file)?;
            file.read_to_string(&mut contents)?;
        // }
    }

    let mut changed = false;

    let contents: String = contents.split("%%").enumerate().map(|(i, content)| {
        if i % 2 == 0 {
            return content.to_string();
        }

        let mut file = File::open(out_dir.clone()+content).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();

        changed = true;

        contents
    }).collect();


    if changed {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&lib_file)?;

        file.write_all(contents.as_bytes()).unwrap();
    }

    Ok(())
}
