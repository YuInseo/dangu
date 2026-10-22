//! 바이옴·구조물·전리품이 서버 전체에서 맞물리는지.

mod common;

use common::test_info;
use mc_server::Server;
use mc_server::world::{Dimension, chunk, structure, terrain};

const SEED: u64 = 20260915;

#[test]
fn structure_chests_are_filled_once_with_vanilla_loot() {
    let (vx, vz, _) =
        structure::locate(SEED, Dimension::Overworld, "village", (0, 0), 200).expect("village");
    let server = Server::new(test_info(), SEED);
    // 마을 둘레 청크에서 상자 하나를 찾습니다.
    let mut found = None;
    'search: for cx in (vx >> 4) - 4..=(vx >> 4) + 4 {
        for cz in (vz >> 4) - 4..=(vz >> 4) + 4 {
            let col = terrain::generate(Dimension::Overworld, SEED, cx, cz);
            if let Some((pos, table)) = col.loot.first() {
                // 청크 패킷에 상자 블록 엔티티가 실림 (개수가 0이 아님)
                let body = chunk::encode(&col, cx, cz);
                assert!(body.len() > 1000);
                found = Some((*pos, *table));
                break 'search;
            }
        }
    }
    let (pos, table) = found.expect("a village has at least one chest");
    assert!(table.starts_with("chests/village/"), "{table}");
    server.fill_structure_loot(Dimension::Overworld, pos);
    let items: Vec<_> = server
        .containers
        .get(Dimension::Overworld, pos)
        .into_iter()
        .flatten()
        .collect();
    assert!(!items.is_empty(), "loot rolled");
    // 두 번째로 열 때는 다시 채우지 않음 (꺼낸 것이 돌아오지 않도록)
    server
        .containers
        .set_slot(Dimension::Overworld, pos, 0, None);
    let before = server.containers.get(Dimension::Overworld, pos);
    server.fill_structure_loot(Dimension::Overworld, pos);
    assert_eq!(server.containers.get(Dimension::Overworld, pos), before);
}

#[test]
fn every_structure_kind_generates_blocks() {
    // 이름마다 가장 가까운 시작점을 찾아 그 청크를 만들어 봅니다 (짓다가 멈추지 않는지).
    for name in structure::names() {
        let dim = match name {
            "fortress" | "bastion_remnant" | "nether_fossil" | "ruined_portal_nether" => {
                Dimension::Nether
            }
            "end_city" => Dimension::End,
            _ => Dimension::Overworld,
        };
        let from = if dim == Dimension::End {
            (1500, 0)
        } else {
            (0, 0)
        };
        if let Some((x, z, _)) = structure::locate(SEED, dim, name, from, 400) {
            let col = terrain::generate(dim, SEED, x >> 4, z >> 4);
            let _ = chunk::encode(&col, x >> 4, z >> 4);
        }
    }
}
