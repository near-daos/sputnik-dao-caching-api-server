use crate::cache::{CachedProposal, ProposalCache};
use anyhow::Result;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::{Orbit, Rocket};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::{Arc, RwLock};

const CACHE_FILE_PATH: &str = "./cache.bin";

pub struct CachePersistence {
    pub proposal_cache: ProposalCache,
}

#[rocket::async_trait]
impl Fairing for CachePersistence {
    fn info(&self) -> Info {
        Info {
            name: "Cache Persistence",
            kind: Kind::Shutdown,
        }
    }

    async fn on_shutdown(&self, _rocket: &Rocket<Orbit>) {
        let cache = self.proposal_cache.read().unwrap();
        let serialized = borsh::to_vec(&*cache).unwrap();

        let mut file = File::create(CACHE_FILE_PATH).expect("Failed to create a file.");
        file.write_all(&serialized).expect("Failed write to file.");
    }
}

pub fn read_cache_from_file() -> Result<ProposalCache> {
    let mut file = File::open(CACHE_FILE_PATH)?;
    let mut serialized = Vec::new();
    file.read_to_end(&mut serialized)?;
    let map: HashMap<(String, u64), CachedProposal> = borsh::from_slice(&serialized)?;

    Ok(Arc::new(RwLock::new(map)))
}
