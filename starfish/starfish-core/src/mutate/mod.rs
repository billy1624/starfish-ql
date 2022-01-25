//! Data manipulation operations

mod edge;
mod node;

pub use edge::*;
pub use node::*;
use sea_orm::{DbConn, DbErr};

use crate::lang::{MutateInsertJson, MutateJson};

/// Mutate node and edge
#[derive(Debug)]
pub struct Mutate;

impl Mutate {
    /// Mutate data in db
    pub async fn mutate(db: &DbConn, mutate_json: MutateJson, upsert: bool) -> Result<(), DbErr> {
        match mutate_json {
            MutateJson::insert(insert_json) => match insert_json {
                MutateInsertJson::node(batch) => {
                    Mutate::insert_node_batch(db, batch, upsert).await?;
                }
                MutateInsertJson::edge(batch) => {
                    Mutate::insert_edge_batch(db, batch).await?;
                }
            },
            MutateJson::update(update_json) => todo!(),
            MutateJson::delete(delete_json) => todo!(),
        };

        Ok(())
    }
}
