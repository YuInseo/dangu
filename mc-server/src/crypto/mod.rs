//! 프로토콜이 쓰는 암호 기본 요소. 정품 인증(RSA 키 교환, AES-CFB8 스트림 암호, 서버 해시
//! SHA-1)과 채팅 서명 검증(SHA-256, RSA PKCS#1 v1.5)에 필요한 만큼만 직접 구현합니다.
//!
//! 이 서버의 다른 부분처럼 외부 크레이트 없이 짰고, 각 요소는 표준 시험 벡터로 검증합니다.
//! (Mojang 세션 서버와의 HTTPS만은 직접 짠 TLS가 위험하므로 rustls를 씁니다 — `auth` 참고.)

pub mod aes;
pub mod base64;
pub mod bigint;
pub mod random;
pub mod rsa;
pub mod sha1;
pub mod sha256;
