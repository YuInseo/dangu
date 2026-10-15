//! 최근 기록. 휴대폰 앱에서는 표준 출력이 어디에도 남지 않으므로, 최근 것을 메모리에 들고 있다가
//! 설정(정보) 화면에 보여 준다. 소리가 안 날 때 무엇이 잘못됐는지 화면 하나로 알 수 있게.

use std::collections::VecDeque;
use std::fmt::Write as _;

use axum::routing::get;
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

use crate::App;

const MAX: usize = 400;

static LINES: Mutex<VecDeque<Line>> = Mutex::new(VecDeque::new());

#[derive(Clone, Debug, Serialize)]
pub struct Line {
    /// 밀리초(유닉스 시간)
    pub at: i64,
    pub level: &'static str,
    pub target: String,
    pub message: String,
}

/// 정보 이상(INFO, WARN, ERROR)의 기록을 모으는 층. 구독자에 덧붙인다.
pub struct RecentLayer;

pub fn layer() -> RecentLayer {
    RecentLayer
}

#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            let _ = write!(self.rest, " {}={value}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.rest, " {}={value:?}", field.name());
        }
    }
}

impl<S: Subscriber> Layer<S> for RecentLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        // 더 자세한 것(DEBUG, TRACE)은 모으지 않는다.
        if *meta.level() > Level::INFO {
            return;
        }
        let mut f = Fields::default();
        event.record(&mut f);
        let level = match *meta.level() {
            Level::ERROR => "error",
            Level::WARN => "warn",
            _ => "info",
        };
        push(Line {
            at: sori_core::model::now_ms(),
            level,
            target: meta.target().to_string(),
            message: format!("{}{}", f.message, f.rest),
        });
    }
}

fn push(line: Line) {
    let mut v = LINES.lock();
    if v.len() >= MAX {
        v.pop_front();
    }
    v.push_back(line);
}

/// 최근 기록(오래된 것부터)
pub fn recent() -> Vec<Line> {
    LINES.lock().iter().cloned().collect()
}

pub fn routes() -> Router<App> {
    Router::new().route("/api/logs", get(|| async { Json(recent()) }))
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::prelude::*;

    #[test]
    fn keeps_recent_lines_with_fields() {
        let sub = tracing_subscriber::registry().with(super::layer());
        tracing::subscriber::with_default(sub, || {
            tracing::info!(device = "스피커", rate = 48_000, "출력 장치를 열었습니다");
            tracing::debug!("이건 남기지 않는다");
            tracing::warn!("장치가 응답하지 않습니다");
        });
        let lines = super::recent();
        let mine: Vec<_> =
            lines.iter().filter(|l| l.message.contains("출력 장치를 열었습니다") || l.message.contains("응답하지")).collect();
        assert_eq!(mine.len(), 2);
        assert_eq!(mine[0].level, "info");
        assert!(mine[0].message.contains("device=스피커") && mine[0].message.contains("rate=48000"), "{}", mine[0].message);
        assert_eq!(mine[1].level, "warn");
        assert!(!lines.iter().any(|l| l.message.contains("남기지 않는다")));
    }
}
