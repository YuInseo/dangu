use serde_json::json;

use super::*;

#[test]
fn links() {
    let v = |id: &str| Some(Link::Video(id.to_string()));
    assert_eq!(parse_link("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s"), v("dQw4w9WgXcQ"));
    assert_eq!(parse_link("https://youtu.be/dQw4w9WgXcQ?si=abc"), v("dQw4w9WgXcQ"));
    assert_eq!(parse_link("youtube.com/shorts/dQw4w9WgXcQ"), v("dQw4w9WgXcQ"));
    assert_eq!(parse_link("https://m.youtube.com/watch?feature=share&v=dQw4w9WgXcQ"), v("dQw4w9WgXcQ"));
    assert_eq!(parse_link("https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RDAMVM"), v("dQw4w9WgXcQ"));
    assert_eq!(parse_link("https://www.youtube.com/embed/dQw4w9WgXcQ"), v("dQw4w9WgXcQ"));
    assert_eq!(parse_link("https://www.youtube.com/live/dQw4w9WgXcQ?feature=share"), v("dQw4w9WgXcQ"));
    assert_eq!(parse_link("dQw4w9WgXcQ"), v("dQw4w9WgXcQ"));
    assert_eq!(
        parse_link("https://www.youtube.com/channel/UCuAXFkgsw1L7xaCfnd5JJOw/videos"),
        Some(Link::Channel("UCuAXFkgsw1L7xaCfnd5JJOw".into()))
    );
    assert_eq!(parse_link("https://www.youtube.com/@NewJeans_official"), Some(Link::Handle("@NewJeans_official".into())));
    assert_eq!(parse_link("@HYBELABELS"), Some(Link::Handle("@HYBELABELS".into())));
    assert_eq!(parse_link("https://www.youtube.com/@%EC%A7%80%EA%BF%8DTV/videos"), Some(Link::Handle("@지꿍TV".into())));
    assert_eq!(
        parse_link("https://www.youtube.com/playlist?list=PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG"),
        Some(Link::Playlist("PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG".into()))
    );
    for bad in [
        "https://tidal.com/browse/album/1",
        "https://www.youtube.com/feed/trending",
        "https://evilyoutube.com/watch?v=dQw4w9WgXcQ",
        "hello",
        "",
    ] {
        assert_eq!(parse_link(bad), None, "{bad}");
    }
}

#[test]
fn durations() {
    assert_eq!(parse_duration("PT4M13S"), Some(253.0));
    assert_eq!(parse_duration("PT1H2M3S"), Some(3723.0));
    assert_eq!(parse_duration("PT45S"), Some(45.0));
    assert_eq!(parse_duration("P1DT2H"), Some(93_600.0));
    assert_eq!(parse_duration("P0D"), Some(0.0));
    assert_eq!(parse_duration("PT"), Some(0.0));
    assert_eq!(parse_duration("4:13"), None);
    assert_eq!(parse_duration("PT4X"), None);
}

#[test]
fn image_urls_are_https_for_google_hosts() {
    assert_eq!(https("//yt3.ggpht.com/a"), "https://yt3.ggpht.com/a");
    assert_eq!(https("http://i.ytimg.com/vi/x/hq.jpg"), "https://i.ytimg.com/vi/x/hq.jpg");
    assert_eq!(https("http://127.0.0.1:9/img.jpg"), "http://127.0.0.1:9/img.jpg");
}

#[test]
fn entities() {
    assert_eq!(unescape("Tom &amp; Jerry&#39;s &quot;Live&quot;"), "Tom & Jerry's \"Live\"");
    assert_eq!(unescape("&#x1F600; &lt;b&gt;"), "😀 <b>");
    assert_eq!(unescape("AT&T & co"), "AT&T & co");
}

fn video_item() -> serde_json::Value {
    json!({
      "kind": "youtube#video",
      "id": "ArmDp-zijuc",
      "snippet": {
        "publishedAt": "2023-07-07T04:00:12Z",
        "channelId": "UCMki_UkHb4qSc0qyEcOHHJw",
        "title": "NewJeans (뉴진스) 'Super Shy' Official MV",
        "description": "NewJeans 2nd EP 'Get Up'",
        "thumbnails": {
          "default": { "url": "https://i.ytimg.com/vi/ArmDp-zijuc/default.jpg", "width": 120, "height": 90 },
          "high": { "url": "https://i.ytimg.com/vi/ArmDp-zijuc/hqdefault.jpg", "width": 480, "height": 360 },
          "maxres": { "url": "https://i.ytimg.com/vi/ArmDp-zijuc/maxresdefault.jpg", "width": 1280, "height": 720 }
        },
        "channelTitle": "HYBE LABELS",
        "categoryId": "10",
        "liveBroadcastContent": "none"
      },
      "contentDetails": { "duration": "PT3M21S", "dimension": "2d", "definition": "hd", "caption": "false" },
      "statistics": { "viewCount": "290123456", "likeCount": "3456789", "favoriteCount": "0", "commentCount": "123456" },
      "status": { "embeddable": true, "privacyStatus": "public" }
    })
}

#[test]
fn videos_channels_and_playlists() {
    let v = parse_video(&video_item()).unwrap();
    assert_eq!(v.id, "ArmDp-zijuc");
    assert_eq!(v.title, "NewJeans (뉴진스) 'Super Shy' Official MV");
    assert_eq!(v.channel_title, "HYBE LABELS");
    assert_eq!(v.duration, Some(201.0));
    assert_eq!(v.views, Some(290_123_456));
    assert_eq!(v.comments, Some(123_456));
    assert_eq!(v.thumb.as_deref(), Some("https://i.ytimg.com/vi/ArmDp-zijuc/maxresdefault.jpg"));
    assert_eq!(v.embeddable, Some(true));
    assert_eq!(v.live, None);
    let j = serde_json::to_value(&v).unwrap();
    assert_eq!(j["channelId"], "UCMki_UkHb4qSc0qyEcOHHJw");
    assert!(j.get("short").is_none());

    let c = parse_channel(&json!({
      "id": "UCMki_UkHb4qSc0qyEcOHHJw",
      "snippet": { "title": "HYBE LABELS", "customUrl": "@hybelabels", "description": "HYBE", "thumbnails": { "medium": { "url": "//yt3.ggpht.com/abc=s240" } } },
      "contentDetails": { "relatedPlaylists": { "likes": "", "uploads": "UUMki_UkHb4qSc0qyEcOHHJw" } },
      "statistics": { "viewCount": "40000000000", "subscriberCount": "75000000", "hiddenSubscriberCount": false, "videoCount": "9000" },
      "brandingSettings": { "image": { "bannerExternalUrl": "https://yt3.googleusercontent.com/banner" } }
    }))
    .unwrap();
    assert_eq!(c.handle.as_deref(), Some("@hybelabels"));
    assert_eq!(c.thumb.as_deref(), Some("https://yt3.ggpht.com/abc=s240"));
    assert_eq!(c.subscribers, Some(75_000_000));
    assert_eq!(c.uploads.as_deref(), Some("UUMki_UkHb4qSc0qyEcOHHJw"));
    assert_eq!(c.banner.as_deref(), Some("https://yt3.googleusercontent.com/banner=w1600"));
    // 구독자 수를 숨긴 채널
    let hidden = parse_channel(
        &json!({ "id": "UCMki_UkHb4qSc0qyEcOHHJw", "statistics": { "subscriberCount": "0", "hiddenSubscriberCount": true } }),
    )
    .unwrap();
    assert_eq!(hidden.subscribers, None);

    let p = parse_playlist(&json!({
      "id": "PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG",
      "snippet": { "title": "K-Pop Hits", "channelId": "UCMki_UkHb4qSc0qyEcOHHJw", "channelTitle": "HYBE LABELS", "thumbnails": { "high": { "url": "https://i.ytimg.com/x.jpg" } } },
      "contentDetails": { "itemCount": 42 }
    }))
    .unwrap();
    assert_eq!(p.count, Some(42));
    assert_eq!(p.channel_title, "HYBE LABELS");
}

#[test]
fn search_results_unescape_titles() {
    let json = json!({
      "kind": "youtube#searchListResponse",
      "nextPageToken": "CBQQAA",
      "pageInfo": { "totalResults": 1000000, "resultsPerPage": 20 },
      "items": [
        { "kind": "youtube#searchResult", "id": { "kind": "youtube#video", "videoId": "ArmDp-zijuc" },
          "snippet": { "publishedAt": "2023-07-07T04:00:12Z", "channelId": "UCMki_UkHb4qSc0qyEcOHHJw", "title": "NewJeans &#39;Super Shy&#39; &amp; more",
                       "description": "", "thumbnails": { "high": { "url": "https://i.ytimg.com/vi/ArmDp-zijuc/hqdefault.jpg" } }, "channelTitle": "HYBE LABELS", "liveBroadcastContent": "none" } },
        { "kind": "youtube#searchResult", "id": { "kind": "youtube#channel", "channelId": "UCMki_UkHb4qSc0qyEcOHHJw" },
          "snippet": { "title": "HYBE LABELS", "description": "HYBE", "thumbnails": { "medium": { "url": "https://yt3.ggpht.com/a" } } } },
        { "kind": "youtube#searchResult", "id": { "kind": "youtube#playlist", "playlistId": "PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG" },
          "snippet": { "title": "Hits", "channelTitle": "HYBE LABELS", "thumbnails": {} } },
        { "kind": "youtube#searchResult", "id": { "kind": "youtube#video", "videoId": "bad" }, "snippet": {} },
        { "kind": "youtube#searchResult", "id": { "kind": "youtube#video", "videoId": "LIVElive123" }, "snippet": { "title": "방송", "liveBroadcastContent": "live" } }
      ]
    });
    let p = page(&json, parse_search_item);
    assert_eq!(p.next.as_deref(), Some("CBQQAA"));
    assert_eq!(p.total, Some(1_000_000));
    assert_eq!(p.items.len(), 4, "모양이 이상한 것은 뺀다");
    let SearchItem::Video(v) = &p.items[0] else { panic!() };
    assert_eq!(v.title, "NewJeans 'Super Shy' & more");
    assert!(matches!(&p.items[1], SearchItem::Channel(c) if c.title == "HYBE LABELS"));
    assert!(matches!(&p.items[2], SearchItem::Playlist(pl) if pl.id.starts_with("PL")));
    assert!(matches!(&p.items[3], SearchItem::Video(v) if v.live.as_deref() == Some("live")));
    let j = serde_json::to_value(&p).unwrap();
    assert_eq!(j["items"][0]["kind"], "video");
    assert_eq!(j["items"][1]["kind"], "channel");
}

#[test]
fn playlist_items_skip_private_and_deleted() {
    let json = json!({ "items": [
        { "snippet": { "title": "Song", "resourceId": { "kind": "youtube#video", "videoId": "ArmDp-zijuc" }, "videoOwnerChannelTitle": "HYBE LABELS",
                       "videoOwnerChannelId": "UCMki_UkHb4qSc0qyEcOHHJw", "thumbnails": { "medium": { "url": "https://i.ytimg.com/m.jpg" } } },
          "contentDetails": { "videoId": "ArmDp-zijuc", "videoPublishedAt": "2023-07-07T04:00:12Z" } },
        { "snippet": { "title": "Private video", "resourceId": { "videoId": "aaaaaaaaaaa" } } },
        { "snippet": { "title": "Deleted video", "resourceId": { "videoId": "bbbbbbbbbbb" } } }
    ] });
    let p = page(&json, parse_playlist_item);
    assert_eq!(p.items.len(), 1);
    assert_eq!(p.items[0].channel_title, "HYBE LABELS");
    assert_eq!(p.items[0].published.as_deref(), Some("2023-07-07T04:00:12Z"));
}

#[test]
fn comments_and_replies() {
    let json = json!({
      "nextPageToken": "QURTSl9p",
      "pageInfo": { "totalResults": 20, "resultsPerPage": 20 },
      "items": [{
        "id": "UgxThread",
        "snippet": {
          "videoId": "ArmDp-zijuc",
          "topLevelComment": { "id": "UgxThread", "snippet": {
            "authorDisplayName": "@fan", "authorProfileImageUrl": "https://yt3.ggpht.com/u", "authorChannelId": { "value": "UCaaaaaaaaaaaaaaaaaaaaaa" },
            "textDisplay": "노래 너무 좋아요 &lt;3", "textOriginal": "노래 너무 좋아요 <3", "likeCount": 1200,
            "publishedAt": "2023-07-07T05:00:00Z", "updatedAt": "2023-07-08T05:00:00Z" } },
          "canReply": true, "totalReplyCount": 31, "isPublic": true
        },
        "replies": { "comments": [ { "id": "UgxThread.reply1", "snippet": { "authorDisplayName": "@other", "textDisplay": "맞아요", "likeCount": 3,
                                      "publishedAt": "2023-07-07T06:00:00Z", "updatedAt": "2023-07-07T06:00:00Z", "parentId": "UgxThread" } } ] }
      }]
    });
    let p = page(&json, parse_thread);
    let c = &p.items[0];
    assert_eq!(c.author, "@fan");
    assert_eq!(c.text, "노래 너무 좋아요 <3");
    assert_eq!(c.likes, 1200);
    assert_eq!(c.replies, 31);
    assert!(c.edited);
    assert_eq!(c.reply_preview.len(), 1);
    assert!(!c.reply_preview[0].edited);
    assert_eq!(c.author_channel_id.as_deref(), Some("UCaaaaaaaaaaaaaaaaaaaaaa"));
}

#[test]
fn errors_people_can_read() {
    let e = |reason: &str, msg: &str| json!({ "error": { "code": 403, "message": msg, "errors": [{ "reason": reason, "message": msg }] } });
    assert!(
        api_error(403, &e("quotaExceeded", "The request cannot be completed because you have exceeded your quota.")).contains("할당량")
    );
    assert!(api_error(403, &e("commentsDisabled", "disabled")).contains("댓글을 막아"));
    assert!(api_error(400, &e("badRequest", "API key not valid. Please pass a valid API key.")).contains("키가 맞지 않습니다"));
    assert!(api_error(403, &e("accessNotConfigured", "YouTube Data API v3 has not been used")).contains("켜 주세요"));
    assert!(api_error(500, &json!(null)).contains("500"));
}

#[test]
fn oembed_without_key() {
    let j = json!({ "title": "Super Shy", "author_name": "HYBE LABELS", "author_url": "https://www.youtube.com/@HYBELABELS", "type": "video",
                    "provider_name": "YouTube", "thumbnail_url": "https://i.ytimg.com/vi/ArmDp-zijuc/hqdefault.jpg" });
    let v = parse_oembed("ArmDp-zijuc", &j).unwrap();
    assert_eq!((v.title.as_str(), v.channel_title.as_str()), ("Super Shy", "HYBE LABELS"));
    assert_eq!(oembed_author(&j), Some(Link::Handle("@HYBELABELS".into())));
}

pub const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns:yt="http://www.youtube.com/xml/schemas/2015" xmlns:media="http://search.yahoo.com/mrss/" xmlns="http://www.w3.org/2005/Atom">
 <link rel="self" href="http://www.youtube.com/feeds/videos.xml?channel_id=UCMki_UkHb4qSc0qyEcOHHJw"/>
 <id>yt:channel:Mki_UkHb4qSc0qyEcOHHJw</id>
 <yt:channelId>Mki_UkHb4qSc0qyEcOHHJw</yt:channelId>
 <title>HYBE LABELS</title>
 <link rel="alternate" href="https://www.youtube.com/channel/UCMki_UkHb4qSc0qyEcOHHJw"/>
 <author>
  <name>HYBE LABELS</name>
  <uri>https://www.youtube.com/channel/UCMki_UkHb4qSc0qyEcOHHJw</uri>
 </author>
 <published>2008-09-12T08:01:05+00:00</published>
 <entry>
  <id>yt:video:ArmDp-zijuc</id>
  <yt:videoId>ArmDp-zijuc</yt:videoId>
  <yt:channelId>UCMki_UkHb4qSc0qyEcOHHJw</yt:channelId>
  <title>NewJeans (뉴진스) &apos;Super Shy&apos; Official MV &amp; more</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=ArmDp-zijuc"/>
  <author>
   <name>HYBE LABELS</name>
   <uri>https://www.youtube.com/channel/UCMki_UkHb4qSc0qyEcOHHJw</uri>
  </author>
  <published>2023-07-07T04:00:12+00:00</published>
  <updated>2024-01-02T03:04:05+00:00</updated>
  <media:group>
   <media:title>NewJeans (뉴진스) &apos;Super Shy&apos; Official MV &amp; more</media:title>
   <media:content url="https://www.youtube.com/v/ArmDp-zijuc?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
   <media:thumbnail url="https://i2.ytimg.com/vi/ArmDp-zijuc/hqdefault.jpg" width="480" height="360"/>
   <media:description>NewJeans 2nd EP &apos;Get Up&apos;</media:description>
   <media:community>
    <media:starRating count="3456789" average="5.00" min="1" max="5"/>
    <media:statistics views="290123456"/>
   </media:community>
  </media:group>
 </entry>
 <entry>
  <id>yt:video:shortSHORT1</id>
  <yt:videoId>shortSHORT1</yt:videoId>
  <yt:channelId>UCMki_UkHb4qSc0qyEcOHHJw</yt:channelId>
  <title>#shorts</title>
  <link rel="alternate" href="https://www.youtube.com/shorts/shortSHORT1"/>
  <author><name>HYBE LABELS</name></author>
  <published>2024-02-01T00:00:00+00:00</published>
  <media:group>
   <media:thumbnail url="https://i2.ytimg.com/vi/shortSHORT1/hqdefault.jpg" width="480" height="360"/>
   <media:description></media:description>
   <media:community><media:statistics views="12"/></media:community>
  </media:group>
 </entry>
</feed>
"#;

#[test]
fn channel_feed() {
    let f = parse_feed("UCMki_UkHb4qSc0qyEcOHHJw", FEED).unwrap();
    assert_eq!(f.title, "HYBE LABELS");
    assert_eq!(f.videos.len(), 2);
    let v = &f.videos[0];
    assert_eq!(v.id, "ArmDp-zijuc");
    assert_eq!(v.title, "NewJeans (뉴진스) 'Super Shy' Official MV & more");
    assert_eq!(v.views, Some(290_123_456));
    assert_eq!(v.likes, Some(3_456_789));
    assert_eq!(v.thumb.as_deref(), Some("https://i2.ytimg.com/vi/ArmDp-zijuc/hqdefault.jpg"));
    assert_eq!(v.published.as_deref(), Some("2023-07-07T04:00:12+00:00"));
    assert_eq!(v.description.as_deref(), Some("NewJeans 2nd EP 'Get Up'"));
    assert_eq!(v.channel_id.as_deref(), Some("UCMki_UkHb4qSc0qyEcOHHJw"));
    assert!(!v.short);
    assert!(f.videos[1].short);
    assert_eq!(f.videos[1].description, None);
    assert_eq!(parse_feed("x", "<html>not a feed</html>"), None);
}

#[test]
fn takeout_subscriptions() {
    let csv = "\u{feff}Channel Id,Channel Url,Channel Title\n\
               UCMki_UkHb4qSc0qyEcOHHJw,http://www.youtube.com/channel/UCMki_UkHb4qSc0qyEcOHHJw,HYBE LABELS\n\
               UCuAXFkgsw1L7xaCfnd5JJOw,http://www.youtube.com/channel/UCuAXFkgsw1L7xaCfnd5JJOw,\"Rick, Astley \"\"Official\"\"\"\n\
               UCMki_UkHb4qSc0qyEcOHHJw,http://www.youtube.com/channel/UCMki_UkHb4qSc0qyEcOHHJw,HYBE LABELS\n\
               \n";
    let got = parse_takeout_csv(csv);
    assert_eq!(
        got,
        vec![
            ("UCMki_UkHb4qSc0qyEcOHHJw".to_string(), "HYBE LABELS".to_string()),
            ("UCuAXFkgsw1L7xaCfnd5JJOw".to_string(), "Rick, Astley \"Official\"".to_string()),
        ]
    );
    // 한국어 머리말
    assert_eq!(parse_takeout_csv("채널 ID,채널 URL,채널 제목\nUCMki_UkHb4qSc0qyEcOHHJw,x,하이브").len(), 1);
}

#[test]
fn newest_first_and_cursor() {
    let v = |id: &str, p: &str| Video { id: id.into(), published: Some(p.into()), ..Default::default() };
    let ids = |items: &[Video]| items.iter().map(|x| x.id.clone()).collect::<Vec<_>>();
    let mut items = vec![
        v("b", "2024-01-01T00:00:00+00:00"),
        v("a", "2025-05-05T00:00:00+00:00"),
        v("c", "2024-01-01T00:00:00+00:00"),
        v("a", "2025-05-05T00:00:00+00:00"),
    ];
    newest_first(&mut items);
    assert_eq!(ids(&items), ["a", "b", "c"], "새 것부터, 같은 영상은 하나");
    let after_b = feed_cursor(&items[1]);
    assert_eq!(after_cursor(&items, &after_b), 2);
    assert_eq!(after_cursor(&items, &feed_cursor(&items[2])), 3, "끝");

    // 그사이 새 영상이 끼어도, 표의 영상이 빠져도 b 다음은 c
    let mut grown = items.clone();
    grown.push(v("z", "2026-01-01T00:00:00+00:00"));
    newest_first(&mut grown);
    assert_eq!(grown[after_cursor(&grown, &after_b)].id, "c");
    let shrunk: Vec<Video> = items.iter().filter(|x| x.id != "b").cloned().collect();
    assert_eq!(shrunk[after_cursor(&shrunk, &after_b)].id, "c");
}

/// 응답하고 조금 뒤 연결을 닫는(그러면서 `Connection: close`를 말하지 않는) 서버. 곧바로 다음 요청을 보내면
/// 쉬던 연결이 아직 열려 보여 다시 쓰다가 끊긴다(연결 유지의 경합).
#[test]
fn reconnects_when_a_kept_connection_was_dropped() {
    use std::io::{BufRead, BufReader, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            while r.read_line(&mut line).unwrap_or(0) > 0 && line != "\r\n" {
                line.clear();
            }
            let body = r#"{"title":"Super Shy","author_name":"HYBE LABELS"}"#;
            let mut s = s;
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
            // 그리고 조금 뒤 닫는다
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let yt = YouTube::with_bases(dir.path(), "http://127.0.0.1:9/youtube/v3", &base);
    for id in ["ArmDp-zijuc", "js1CtxSY38I", "pSUydWEqKwE"] {
        let v = yt.video(id).unwrap_or_else(|e| panic!("{id}: {e:#}"));
        assert_eq!(v.video.title, "Super Shy");
    }
}
