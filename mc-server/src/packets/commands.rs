//! `Commands` 패킷: 클라이언트에 명령어 문법 트리를 알려 줍니다. 클라이언트는 이걸로
//! 자동 완성을 하고, 트리에 없는 명령어는 빨갛게 표시합니다.
//!
//! 노드 플래그: 하위 2비트 = 종류(0 루트, 1 리터럴, 2 인자), 0x04 = 여기서 끝나도 실행 가능.

use crate::protocol::types::write_string;
use crate::protocol::varint::write_varint;

enum Kind {
    Root,
    Literal(std::borrow::Cow<'static, str>),
    Argument {
        name: &'static str,
        parser: i32,
        properties: &'static [u8],
        /// 서버에 자동 완성을 물어보는가 (`minecraft:ask_server`).
        ask_server: bool,
    },
}

struct Node {
    kind: Kind,
    executable: bool,
    children: Vec<usize>,
}

pub struct CommandTree {
    nodes: Vec<Node>,
}

/// 루트 노드의 번호.
pub const ROOT: usize = 0;

impl Default for CommandTree {
    fn default() -> Self {
        Self {
            nodes: vec![Node {
                kind: Kind::Root,
                executable: false,
                children: Vec::new(),
            }],
        }
    }
}

impl CommandTree {
    fn push(&mut self, parent: usize, kind: Kind, executable: bool) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            executable,
            children: Vec::new(),
        });
        self.nodes[parent].children.push(id);
        id
    }

    pub fn literal(
        &mut self,
        parent: usize,
        name: impl Into<std::borrow::Cow<'static, str>>,
        executable: bool,
    ) -> usize {
        self.push(parent, Kind::Literal(name.into()), executable)
    }

    /// 속성이 없는 파서만 받습니다 (`game_profile`, `message` 등).
    pub fn argument(
        &mut self,
        parent: usize,
        name: &'static str,
        parser: i32,
        executable: bool,
    ) -> usize {
        self.push(
            parent,
            Kind::Argument {
                name,
                parser,
                properties: &[],
                ask_server: false,
            },
            executable,
        )
    }

    /// 속성이 있는 파서 (예: `brigadier:integer`의 범위 플래그 바이트).
    pub fn argument_with(
        &mut self,
        parent: usize,
        name: &'static str,
        parser: i32,
        properties: &'static [u8],
        executable: bool,
    ) -> usize {
        self.push(
            parent,
            Kind::Argument {
                name,
                parser,
                properties,
                ask_server: false,
            },
            executable,
        )
    }

    /// 후보를 서버가 주는 인자: 클라이언트가 칠 때마다 `Command Suggestion`을 보냅니다.
    pub fn argument_ask_server(
        &mut self,
        parent: usize,
        name: &'static str,
        parser: i32,
        properties: &'static [u8],
        executable: bool,
    ) -> usize {
        self.push(
            parent,
            Kind::Argument {
                name,
                parser,
                properties,
                ask_server: true,
            },
            executable,
        )
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_varint(&mut out, self.nodes.len() as i32);
        for node in &self.nodes {
            let kind = match &node.kind {
                Kind::Root => 0,
                Kind::Literal(_) => 1,
                Kind::Argument { .. } => 2,
            };
            let ask = matches!(
                node.kind,
                Kind::Argument {
                    ask_server: true,
                    ..
                }
            );
            out.push(kind | if node.executable { 0x04 } else { 0 } | if ask { 0x10 } else { 0 });
            write_varint(&mut out, node.children.len() as i32);
            for &c in &node.children {
                write_varint(&mut out, c as i32);
            }
            match &node.kind {
                Kind::Root => {}
                Kind::Literal(name) => write_string(&mut out, name),
                Kind::Argument {
                    name,
                    parser,
                    properties,
                    ask_server,
                } => {
                    write_string(&mut out, name);
                    write_varint(&mut out, *parser);
                    out.extend_from_slice(properties);
                    if *ask_server {
                        write_string(&mut out, "minecraft:ask_server");
                    }
                }
            }
        }
        write_varint(&mut out, ROOT as i32);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_literal_and_argument() {
        let mut t = CommandTree::default();
        let tp = t.literal(ROOT, "tp", false);
        t.argument(tp, "target", 7, true);
        assert_eq!(
            t.encode(),
            [
                3, // 노드 3개
                0x00, 1, 1, // 루트 → 1
                0x01, 1, 2, 2, b't', b'p', // 리터럴 tp → 2
                0x06, 0, 6, b't', b'a', b'r', b'g', b'e', b't', 7, // 실행 가능한 인자
                0, // 루트 번호
            ]
        );
    }
}
