//! 담비 서버: 마스터 노드(프록시 + CLI) 하나가 컨텐츠 월드마다 독립된 자식 프로세스를 띄우고,
//! 유닉스 소켓 IPC로 묶습니다. 자식 하나 = 인스턴스 하나 = 오버월드·네더·엔드 세 차원.
//!
//! - [`master`] — 상태, IPC 허브, 감시 (램 할당량), SharedData 쓰기
//! - [`proxy`] — 플레이어 연결을 받아 인스턴스로 잇는 루프
//! - [`cli`] — 관리자 명령어 (`instance start rpg --ram 4G`)
//! - [`process`] — 자식 프로세스 띄우기, cgroup v2 / RSS 감시
//! - [`ipc`] — 마스터 ↔ 자식 메시지
//! - [`shared`] — SharedData (3중 재화, 플레이 시간, 생활 직업)
//! - [`child`] — 자식 프로세스 쪽 연결과 API

pub mod child;
pub mod cli;
pub mod ipc;
pub mod master;
pub mod process;
pub mod proxy;
pub mod shared;
