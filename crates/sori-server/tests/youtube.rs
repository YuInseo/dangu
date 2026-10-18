//! 유튜브를 서버 끝에서 끝까지: 가짜 유튜브(Data API, oEmbed, 채널 RSS) → 소리 서버 → 화면이 부르는 API.
//! 키 없이 되는 것(구독, 구독 피드, 영상 제목)과 키가 있어야 되는 것(검색, 댓글, 채널 정보)을 가른다.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};
use sori_core::youtube::YouTube;
use sori_core::{Config, Store};
use sori_server::App;

const KEY: &str = "AIzaSyTEST_key_0123456789abcdefghijklm";
const VIDEO: &str = "ArmDp-zijuc";
const CHANNEL: &str = "UCMki_UkHb4qSc0qyEcOHHJw";
/// 올린 사람이 유튜브 밖 재생을 막은 영상
const NO_EMBED: &str = "noEmbedVid0";

const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns:yt="http://www.youtube.com/xml/schemas/2015" xmlns:media="http://search.yahoo.com/mrss/" xmlns="http://www.w3.org/2005/Atom">
 <title>HYBE LABELS</title>
 <entry>
  <yt:videoId>ArmDp-zijuc</yt:videoId>
  <yt:channelId>UCMki_UkHb4qSc0qyEcOHHJw</yt:channelId>
  <title>Super Shy</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=ArmDp-zijuc"/>
  <author><name>HYBE LABELS</name></author>
  <published>2023-07-07T04:00:12+00:00</published>
  <media:group>
   <media:thumbnail url="https://i2.ytimg.com/vi/ArmDp-zijuc/hqdefault.jpg" width="480" height="360"/>
   <media:community><media:statistics views="290123456"/></media:community>
  </media:group>
 </entry>
 <entry>
  <yt:videoId>newerVIDEO1</yt:videoId>
  <title>New one</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=newerVIDEO1"/>
  <author><name>HYBE LABELS</name></author>
  <published>2024-03-01T00:00:00+00:00</published>
 </entry>
</feed>"#;

fn video_json(id: &str) -> Value {
    json!({
      "id": id,
      "snippet": { "publishedAt": "2023-07-07T04:00:12Z", "channelId": CHANNEL, "title": "NewJeans 'Super Shy' Official MV", "description": "Get Up",
                   "thumbnails": { "high": { "url": format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg") } }, "channelTitle": "HYBE LABELS", "liveBroadcastContent": "none" },
      "contentDetails": { "duration": "PT3M21S" },
      "statistics": { "viewCount": "290123456", "likeCount": "3456789", "commentCount": "123456" },
      "status": { "embeddable": true }
    })
}

fn channel_json() -> Value {
    json!({
      "id": CHANNEL,
      "snippet": { "title": "HYBE LABELS", "customUrl": "@hybelabels", "thumbnails": { "medium": { "url": "https://yt3.ggpht.com/avatar" } } },
      "contentDetails": { "relatedPlaylists": { "uploads": "UUMki_UkHb4qSc0qyEcOHHJw" } },
      "statistics": { "subscriberCount": "75000000", "videoCount": "9000", "viewCount": "1", "hiddenSubscriberCount": false }
    })
}

fn api(path: &str, q: &HashMap<String, String>) -> (u16, String) {
    let ok = |v: Value| (200, v.to_string());
    if q.get("key").map(String::as_str) != Some(KEY) {
        let e = json!({ "error": { "code": 400, "message": "API key not valid. Please pass a valid API key.", "errors": [{ "reason": "badRequest" }] } });
        return (400, e.to_string());
    }
    match path {
        "videos" => {
            if q.get("chart").is_some() {
                return ok(json!({ "items": [video_json(VIDEO)], "nextPageToken": "POP2" }));
            }
            let ids = q.get("id").cloned().unwrap_or_default();
            let items: Vec<Value> = ids
                .split(',')
                .filter(|i| !i.is_empty() && *i != "goneVIDEO00")
                .map(|i| {
                    let mut v = video_json(i);
                    v["status"]["embeddable"] = json!(i != NO_EMBED);
                    v
                })
                .collect();
            ok(json!({ "items": items }))
        }
        "search" => ok(json!({
            "nextPageToken": "S2",
            "pageInfo": { "totalResults": 1000 },
            "items": [
                { "id": { "kind": "youtube#channel", "channelId": CHANNEL }, "snippet": { "title": "HYBE LABELS" } },
                { "id": { "kind": "youtube#video", "videoId": VIDEO }, "snippet": { "title": "NewJeans &#39;Super Shy&#39;", "channelTitle": "HYBE LABELS", "channelId": CHANNEL } }
            ]
        })),
        "channels" => {
            if q.get("forHandle").map(String::as_str) == Some("@hybelabels") {
                return ok(json!({ "items": [{ "id": CHANNEL }] }));
            }
            if q.get("forHandle").is_some() {
                return ok(json!({}));
            }
            ok(json!({ "items": [channel_json()] }))
        }
        "playlistItems" => ok(json!({ "items": [
            { "snippet": { "title": "Super Shy", "resourceId": { "videoId": VIDEO }, "videoOwnerChannelTitle": "HYBE LABELS", "videoOwnerChannelId": CHANNEL },
              "contentDetails": { "videoId": VIDEO, "videoPublishedAt": "2023-07-07T04:00:12Z" } },
            { "snippet": { "title": "Private video", "resourceId": { "videoId": "privateVID0" } } }
        ], "nextPageToken": "U2" })),
        "commentThreads" => {
            if q.get("videoId").map(String::as_str) == Some("noComments0") {
                let e = json!({ "error": { "code": 403, "message": "disabled comments", "errors": [{ "reason": "commentsDisabled" }] } });
                return (403, e.to_string());
            }
            ok(json!({ "nextPageToken": "C2", "items": [{
                "snippet": { "totalReplyCount": 2, "topLevelComment": { "id": "Ugx1", "snippet": {
                    "authorDisplayName": "@fan", "textDisplay": "좋아요", "likeCount": 10, "publishedAt": "2023-07-07T05:00:00Z", "updatedAt": "2023-07-07T05:00:00Z" } } }
            }] }))
        }
        "comments" => {
            ok(json!({ "items": [{ "id": "Ugx1.a", "snippet": { "authorDisplayName": "@a", "textDisplay": "저도요", "likeCount": 1 } }] }))
        }
        "playlists" => ok(
            json!({ "items": [{ "id": "PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG", "snippet": { "title": "Hits", "channelTitle": "HYBE LABELS" }, "contentDetails": { "itemCount": 2 } }] }),
        ),
        _ => (404, "{}".into()),
    }
}

fn web(path: &str, q: &HashMap<String, String>) -> (u16, String) {
    match path {
        "oembed" => {
            let url = q.get("url").cloned().unwrap_or_default();
            if url.ends_with(VIDEO) {
                (200, json!({ "title": "Super Shy", "author_name": "HYBE LABELS", "author_url": format!("https://www.youtube.com/channel/{CHANNEL}") }).to_string())
            } else if url.ends_with(NO_EMBED) {
                (401, "Unauthorized".into())
            } else {
                (404, "Not Found".into())
            }
        }
        "feeds/videos.xml" if q.get("channel_id").map(String::as_str) == Some(CHANNEL) => (200, FEED.into()),
        _ => (404, "".into()),
    }
}

/// 가짜 유튜브. `/youtube/v3/…`는 Data API, 나머지는 www.youtube.com.
fn serve(calls: Arc<AtomicUsize>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let calls = calls.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                }
                let target = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (path, query) = target.split_once('?').unwrap_or((&target, ""));
                let q: HashMap<String, String> = form_urlencoded::parse(query.as_bytes()).into_owned().collect();
                let (status, body) = match path.strip_prefix("/youtube/v3/") {
                    Some(p) => {
                        calls.fetch_add(1, Ordering::SeqCst);
                        api(p, &q)
                    }
                    None => web(path.trim_start_matches('/'), &q),
                };
                let mut s = s;
                let _ = write!(
                    s,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    base
}

fn app(base: &str, dir: &tempfile::TempDir) -> App {
    let music = dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let store = Store::open(Config::new(&music, dir.path().join("data")));
    let mut app = App::new(store, String::new());
    app.youtube = Arc::new(YouTube::with_bases(&dir.path().join("data"), &format!("{base}/youtube/v3"), base));
    app
}

fn start(app: App) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let (otx, orx) = tokio::sync::oneshot::channel();
            tokio::spawn(sori_server::serve(app, "127.0.0.1:0".parse().unwrap(), Some(otx)));
            tx.send(orx.await.unwrap()).unwrap();
            std::future::pending::<()>().await;
        });
    });
    format!("http://{}", rx.recv().unwrap())
}

fn client() -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).build().new_agent()
}

fn get(url: &str) -> (u16, Value) {
    let mut r = client().get(url).call().unwrap();
    let s = r.status().as_u16();
    (s, serde_json::from_slice(&r.body_mut().read_to_vec().unwrap()).unwrap_or(Value::Null))
}

fn send(method: &str, url: &str, body: Option<String>) -> (u16, Value) {
    let c = client();
    let mut r = match (method, body) {
        ("DELETE", _) => c.delete(url).call().unwrap(),
        (_, Some(b)) => c.post(url).header("content-type", "application/json").send(b).unwrap(),
        (_, None) => c.post(url).send_empty().unwrap(),
    };
    let s = r.status().as_u16();
    (s, serde_json::from_slice(&r.body_mut().read_to_vec().unwrap()).unwrap_or(Value::Null))
}

#[test]
fn youtube_with_and_without_a_key() {
    let calls = Arc::new(AtomicUsize::new(0));
    let yt = serve(calls.clone());
    let dir = tempfile::tempdir().unwrap();
    let base = start(app(&yt, &dir));

    // 키 없이
    let (_, st) = get(&format!("{base}/api/youtube"));
    assert_eq!(st["hasKey"], false);
    let (s, e) = get(&format!("{base}/api/youtube/search?q=newjeans"));
    assert_eq!((s, &e["needKey"]), (412, &json!(true)), "{e}");
    let (s, v) = get(&format!("{base}/api/youtube/video/{VIDEO}"));
    assert_eq!(s, 200, "{v}");
    assert_eq!((v["limited"].as_bool(), v["video"]["title"].as_str()), (Some(true), Some("Super Shy")));
    assert_eq!(v["channel"]["id"], CHANNEL, "oEmbed의 채널 주소에서 채널을 안다");
    assert_eq!(v["subscribed"], false);
    assert_eq!(get(&format!("{base}/api/youtube/video/nosuchvide0")).0, 404);
    assert_eq!(get(&format!("{base}/api/youtube/video/..%2Fx")).0, 400, "이상한 ID는 묻지도 않는다");
    let (s, v) = get(&format!("{base}/api/youtube/video/{NO_EMBED}"));
    assert_eq!((s, &v["video"]["embeddable"], &v["limited"]), (200, &json!(false), &json!(true)), "막힌 영상은 막혔다고 알린다: {v}");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "키 없이 Data API를 부르지 않는다");

    // 구독과 구독 피드(RSS)
    let (s, r) =
        send("POST", &format!("{base}/api/youtube/subscriptions"), Some(json!({ "id": CHANNEL, "title": "HYBE LABELS" }).to_string()));
    assert_eq!((s, &r["new"]), (200, &json!(true)));
    let (_, f) = get(&format!("{base}/api/youtube/feed"));
    let ids: Vec<&str> = f["items"].as_array().unwrap().iter().map(|v| v["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["newerVIDEO1", VIDEO], "새 것부터");
    let (s, ch) = get(&format!("{base}/api/youtube/channel/{CHANNEL}"));
    assert_eq!((s, &ch["limited"], &ch["subscribed"]), (200, &json!(true), &json!(true)), "{ch}");
    assert_eq!(ch["channel"]["title"], "HYBE LABELS");
    assert_eq!(ch["videos"]["items"].as_array().map(Vec::len), Some(2));

    // 키 넣기: 모양이 틀리거나 유튜브가 거절하면 저장하지 않는다.
    let (s, e) = send("POST", &format!("{base}/api/youtube/key"), Some(json!({ "key": "short" }).to_string()));
    assert_eq!(s, 400, "{e}");
    let (s, e) =
        send("POST", &format!("{base}/api/youtube/key"), Some(json!({ "key": "AIzaSyWRONG_key_0123456789abcdefghijk" }).to_string()));
    assert_eq!(s, 400);
    assert!(e["error"].as_str().unwrap().contains("키가 맞지 않습니다"), "{e}");
    let (s, k) = send("POST", &format!("{base}/api/youtube/key"), Some(json!({ "key": KEY }).to_string()));
    assert_eq!((s, &k["hasKey"]), (200, &json!(true)), "{k}");
    assert_eq!(k["keyHint"], "…jklm");

    // 키로: 검색(채널·영상, 제목 엔티티 풀고 길이·조회수·구독자 채움)
    let (s, r) = get(&format!("{base}/api/youtube/search?q=newjeans"));
    assert_eq!(s, 200, "{r}");
    assert_eq!(r["next"], "S2");
    assert_eq!(r["items"][0]["kind"], "channel");
    assert_eq!(r["items"][0]["subscribers"], 75_000_000);
    assert_eq!(r["items"][1]["kind"], "video");
    assert_eq!(r["items"][1]["duration"], 201.0);
    assert_eq!(r["items"][1]["views"], 290_123_456);

    let (_, v) = get(&format!("{base}/api/youtube/video/{VIDEO}"));
    assert_eq!(v["limited"], false);
    assert_eq!(v["video"]["likes"], 3_456_789);
    assert_eq!(v["channel"]["subscribers"], 75_000_000);
    assert_eq!(v["subscribed"], true);
    assert_eq!(get(&format!("{base}/api/youtube/video/goneVIDEO00")).0, 404, "지워진 영상");
    let (_, v) = get(&format!("{base}/api/youtube/video/{NO_EMBED}"));
    assert_eq!((&v["video"]["embeddable"], &v["video"]["title"]), (&json!(false), &json!("NewJeans 'Super Shy' Official MV")));

    let (s, c) = get(&format!("{base}/api/youtube/comments/{VIDEO}?order=time"));
    assert_eq!(s, 200, "{c}");
    assert_eq!(
        (c["items"][0]["text"].as_str(), c["items"][0]["replies"].as_u64(), c["next"].as_str()),
        (Some("좋아요"), Some(2), Some("C2"))
    );
    let (s, e) = get(&format!("{base}/api/youtube/comments/noComments0"));
    assert_eq!(s, 403);
    assert!(e["error"].as_str().unwrap().contains("댓글을 막아"), "{e}");
    let (_, rp) = get(&format!("{base}/api/youtube/replies/Ugx1"));
    assert_eq!(rp["items"][0]["text"], "저도요");

    let (_, ch) = get(&format!("{base}/api/youtube/channel/{CHANNEL}"));
    assert_eq!(ch["limited"], false);
    assert_eq!(ch["channel"]["handle"], "@hybelabels");
    assert_eq!(ch["videos"]["items"].as_array().map(Vec::len), Some(1), "비공개 영상은 뺀다");
    assert_eq!(ch["videos"]["items"][0]["duration"], 201.0);
    let (_, more) = get(&format!("{base}/api/youtube/channel/{CHANNEL}/videos?page=U2"));
    assert_eq!(more["items"].as_array().map(Vec::len), Some(1));
    let (_, pls) = get(&format!("{base}/api/youtube/channel/{CHANNEL}/playlists"));
    assert_eq!(pls["items"][0]["count"], 2);
    let (_, pl) = get(&format!("{base}/api/youtube/playlist/PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG"));
    assert_eq!(pl["playlist"]["title"], "Hits");
    assert_eq!(pl["videos"]["items"][0]["id"], VIDEO);
    let (_, pop) = get(&format!("{base}/api/youtube/popular?region=kr"));
    assert_eq!(pop["items"][0]["id"], VIDEO);

    // 링크
    let resolve = |url: &str| send("POST", &format!("{base}/api/youtube/resolve"), Some(json!({ "url": url }).to_string()));
    assert_eq!(resolve("https://youtu.be/ArmDp-zijuc?si=x").1, json!({ "kind": "video", "id": VIDEO }));
    assert_eq!(resolve("https://www.youtube.com/@hybelabels").1, json!({ "kind": "channel", "id": CHANNEL }));
    assert_eq!(resolve("https://www.youtube.com/@nobody_here").0, 404);
    assert_eq!(resolve("https://tidal.com/album/1").0, 400);

    // 테이크아웃 가져오기, 구독 끊기
    let csv = format!("Channel Id,Channel Url,Channel Title\n{CHANNEL},x,HYBE LABELS\nUCuAXFkgsw1L7xaCfnd5JJOw,x,Rick Astley\n");
    let mut r = client().post(&format!("{base}/api/youtube/subscriptions/import")).header("content-type", "text/csv").send(csv).unwrap();
    let j: Value = serde_json::from_slice(&r.body_mut().read_to_vec().unwrap()).unwrap();
    assert_eq!((j["found"].as_u64(), j["added"].as_u64(), j["count"].as_u64()), (Some(2), Some(1), Some(2)), "{j}");
    let (_, subs) = get(&format!("{base}/api/youtube/subscriptions"));
    assert_eq!(subs.as_array().map(Vec::len), Some(2));
    let (_, r) = send("DELETE", &format!("{base}/api/youtube/subscriptions/UCuAXFkgsw1L7xaCfnd5JJOw"), None);
    assert_eq!(r["removed"], true);
    // 피드를 못 읽는 채널이 있어도 나머지는 보인다
    send("POST", &format!("{base}/api/youtube/subscriptions"), Some(json!({ "id": "UCuAXFkgsw1L7xaCfnd5JJOw", "title": "x" }).to_string()));
    let (_, f) = get(&format!("{base}/api/youtube/feed"));
    assert_eq!(f["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(f["failed"], json!(["UCuAXFkgsw1L7xaCfnd5JJOw"]));

    // 다시 켜도 키와 구독이 남는다
    let again = start(app(&yt, &dir));
    let (_, st) = get(&format!("{again}/api/youtube"));
    assert_eq!((st["hasKey"].as_bool(), st["subscriptions"].as_u64()), (Some(true), Some(2)));
    let (_, k) = send("DELETE", &format!("{again}/api/youtube/key"), None);
    assert_eq!(k["hasKey"], false);
}
