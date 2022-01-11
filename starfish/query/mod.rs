//! Graph query engine

mod executor;
mod worker;

use self::executor::Executor;
use sea_orm::{ConnectionTrait, DbConn, DbErr, FromQueryResult, Order, Statement};
use sea_query::{Alias, Expr, SelectStatement};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::mem;

const BATCH_SIZE: usize = 300;
const DEBUG: bool = false;

/// Query graph data
#[derive(Debug)]
pub struct Query;

/// Graph data
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphData {
    /// Graph node data
    nodes: Vec<GraphNodeData>,
    /// Link data
    links: Vec<GraphLinkData>,
}

/// Graph node data
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct GraphNodeData {
    /// Name of node
    id: String,
    /// Weight
    weight: i32,
}

/// Tree data
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TreeData {
    /// Tree node data
    nodes: Vec<TreeNodeData>,
    /// Link data
    links: Vec<TreeLinkData>,
}

/// Tree node data
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct TreeNodeData {
    /// Name of node
    id: String,
    /// Node type
    r#type: TreeNodeType,
}

/// Denotes which side a node belongs to, relative to the **root** node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize_repr, Serialize_repr)]
#[repr(u8)]
pub enum TreeNodeType {
    /// Centered
    Root = 0,
    /// To the Left
    Dependency = 1,
    /// To the Right
    Dependent = 2,
}

/// Graph link data
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct GraphLinkData {
    /// Source node
    source: String,
    /// Target node
    target: String,
}

/// Tree link data
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct TreeLinkData {
    /// Source node
    source: String,
    /// Target node
    target: String,
    /// Edge type
    r#type: TreeNodeType,
}

#[derive(Debug, Clone, FromQueryResult)]
struct Node {
    name: String,
    in_conn: i32,
}

#[derive(Debug, Clone, FromQueryResult)]
struct Link {
    from_node: String,
    to_node: String,
}

impl Query {
    /// Get graph
    pub async fn get_graph(
        db: &DbConn,
        top_n: i32,
        limit: i32,
        depth: i32,
    ) -> Result<GraphData, DbErr> {
        Executor::new(db).get_graph(top_n, limit, depth).await
    }

    /// Get tree
    pub async fn get_tree(
        db: &DbConn,
        root_node: String,
        limit: i32,
        depth: i32,
    ) -> Result<TreeData, DbErr> {
        Executor::new(db).get_tree(root_node, limit, depth).await
    }
}

async fn traverse<N, L, SN, SL, CN, CL>(
    db: &DbConn,
    tree_node_type: TreeNodeType,
    select_nodes: SN,
    select_links: SL,
    convert_node: CN,
    convert_link: CL,
) -> Result<(Vec<N>, Vec<L>), DbErr>
where
    SN: FnOnce(&mut SelectStatement),
    SL: FnOnce(&SelectStatement) -> Vec<SelectStatement>,
    CN: Fn(Node) -> N,
    CL: Fn(Link) -> L,
{
    let builder = db.get_database_backend();
    let mut pending_nodes = Vec::new();
    let mut node_stmt = sea_query::Query::select();
    node_stmt
        .columns([Alias::new("name"), Alias::new("in_conn")])
        .from(Alias::new("node_crate"));
    let mut edge_stmt = sea_query::Query::select();
    let join_col = match tree_node_type {
        TreeNodeType::Root => "",
        TreeNodeType::Dependency => "from_node",
        TreeNodeType::Dependent => "to_node",
    };
    edge_stmt
        .columns([Alias::new("from_node"), Alias::new("to_node")])
        .from(Alias::new("edge_depends"))
        .inner_join(
            Alias::new("node_crate"),
            Expr::tbl(Alias::new("node_crate"), Alias::new("name"))
                .equals(Alias::new("edge_depends"), Alias::new(join_col)),
        )
        .order_by(
            (Alias::new("node_crate"), Alias::new("in_conn")),
            Order::Desc,
        );

    let edge_stmts = select_links(&edge_stmt);
    let links = if !edge_stmts.is_empty() {
        let edge_stmts: Vec<String> = edge_stmts
            .iter()
            .map(|stmt| builder.build(stmt).to_string())
            .collect();
        let union_select = format!("({})", edge_stmts.join(") UNION ("));
        let stmt = Statement::from_string(builder, union_select);
        let res_links = Link::find_by_statement(stmt).all(db).await?;
        pending_nodes = res_links
            .iter()
            .map(|edge| match tree_node_type {
                TreeNodeType::Root => unreachable!(),
                TreeNodeType::Dependency => edge.from_node.clone(),
                TreeNodeType::Dependent => edge.to_node.clone(),
            })
            .collect();
        res_links.into_iter().map(convert_link).collect()
    } else {
        vec![]
    };

    let node_stmt = if matches!(tree_node_type, TreeNodeType::Root) {
        select_nodes(&mut node_stmt);
        node_stmt
    } else {
        let target_nodes = mem::take(&mut pending_nodes);
        node_stmt.and_where(Expr::col(Alias::new("name")).is_in(target_nodes));
        node_stmt
    };

    let res_nodes = Node::find_by_statement(builder.build(&node_stmt))
        .all(db)
        .await?;
    let nodes = res_nodes.into_iter().map(convert_node).collect();

    Ok((nodes, links))
}

fn select_top_n_node(stmt: &mut SelectStatement, top_n: i32) {
    stmt.order_by(Alias::new("in_conn"), Order::Desc)
        .limit(top_n as u64);
}

fn select_root_node(stmt: &mut SelectStatement, root_node: String) {
    stmt.and_where(Expr::col(Alias::new("name")).eq(root_node));
}

fn select_top_n_edge(
    stmt: &SelectStatement,
    limit: i32,
    nodes: Vec<String>,
    tree_node_type: TreeNodeType,
) -> Vec<SelectStatement> {
    if nodes.is_empty() {
        vec![]
    } else {
        nodes
            .into_iter()
            .map(|node| {
                let mut stmt = stmt.clone();
                let col = match tree_node_type {
                    TreeNodeType::Root => unreachable!(),
                    TreeNodeType::Dependency => "to_node",
                    TreeNodeType::Dependent => "from_node",
                };
                stmt.and_where(Expr::col(Alias::new(col)).eq(node))
                    .limit(limit as u64);
                stmt
            })
            .collect()
    }
}

fn into_graph_node(node: Node) -> GraphNodeData {
    GraphNodeData {
        id: node.name,
        weight: node.in_conn,
    }
}

fn into_graph_link(link: Link) -> GraphLinkData {
    GraphLinkData {
        source: link.from_node,
        target: link.to_node,
    }
}

fn into_tree_node(node: Node, r#type: TreeNodeType) -> TreeNodeData {
    TreeNodeData {
        id: node.name,
        r#type,
    }
}

fn into_tree_link(link: Link, r#type: TreeNodeType) -> TreeLinkData {
    TreeLinkData {
        source: link.from_node,
        target: link.to_node,
        r#type,
    }
}
