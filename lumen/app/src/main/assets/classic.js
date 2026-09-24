/*
 * Lumen 클래식 — 옛날 디스코드 모바일 서랍을 위한 데이터 다리.
 *
 * 디스코드 웹 클라이언트가 이미 들고 있는 상태(Flux 스토어: 서버·채널·읽음 표시)를 읽어
 * 앱의 네이티브 서랍으로 넘긴다. 디스코드 서버에 따로 요청하지 않고, 토큰도 건드리지 않는다.
 * 채널 이동은 클라이언트 자신의 라우터로 한다.
 *
 * 앱 → 페이지:  __lumenChannels(guildIdOr@me) → JSON 문자열,  __lumenNav(path)
 * 페이지 → 앱:  LumenBridge.state(JSON)  — 바뀔 때마다(모아서)
 */
(function () {
  if (window.__lumenClassic) return;
  window.__lumenClassic = true;
  var B = window.LumenBridge;

  var req = null;
  var stores = {};
  var transitionTo = null;
  var ready = false;

  function safe(f, d) { try { var v = f(); return v === undefined ? d : v; } catch (e) { return d; } }

  function getReq() {
    if (req) return req;
    var chunk = window.webpackChunkdiscord_app;
    if (!chunk || typeof chunk.push !== 'function') return null;
    try {
      chunk.push([[Symbol('lumen')], {}, function (r) { req = r; }]);
      if (typeof chunk.pop === 'function') chunk.pop();
    } catch (e) {}
    return req;
  }

  function each(fn) {
    var r = getReq();
    if (!r || !r.c) return;
    var cache = r.c;
    for (var id in cache) {
      var m = cache[id];
      var ex = m && m.exports;
      if (!ex) continue;
      if (fn(ex)) return;
      if (typeof ex === 'object') {
        var keys;
        try { keys = Object.keys(ex); } catch (e) { continue; }
        for (var i = 0; i < keys.length; i++) {
          var v;
          try { v = ex[keys[i]]; } catch (e) { continue; }
          if (v && fn(v)) return;
        }
      }
    }
  }

  function scanStores() {
    each(function (v) {
      if ((typeof v === 'object' || typeof v === 'function') &&
          typeof v.getName === 'function' && typeof v.addChangeListener === 'function') {
        var n = safe(function () { return v.getName(); }, null);
        if (n && !stores[n]) stores[n] = v;
      }
      return false;
    });
    return !!(stores.GuildStore && stores.ChannelStore);
  }

  function findTransition() {
    if (transitionTo) return transitionTo;
    each(function (v) {
      if (typeof v !== 'function') return false;
      var src = safe(function () { return Function.prototype.toString.call(v); }, '');
      if (src.indexOf('transitionTo - Transitioning to') >= 0) { transitionTo = v; return true; }
      return false;
    });
    return transitionTo;
  }

  var actions = null;
  function findActions() {
    if (actions) return actions;
    each(function (v) {
      if (typeof v === 'object' && typeof v.sendMessage === 'function' && typeof v.fetchMessages === 'function') {
        actions = v; return true;
      }
      return false;
    });
    return actions;
  }

  function me() {
    var u = safe(function () { return stores.UserStore.getCurrentUser(); }, null);
    if (!u) return null;
    return { id: String(u.id), name: String(u.globalName || u.username || ''), avatar: u.avatar ? String(u.avatar) : '' };
  }

  function userName(uid, gid) {
    var m = gid ? safe(function () { return stores.GuildMemberStore.getMember(gid, uid); }, null) : null;
    if (m && m.nick) return String(m.nick);
    var u = safe(function () { return stores.UserStore.getUser(uid); }, null);
    return u ? String(u.globalName || u.username || '') : '';
  }

  // 멘션·채널·사용자 이모지를 사람이 읽는 글로
  function plain(text, gid) {
    if (!text) return '';
    return String(text)
      .replace(/<@!?(\d+)>/g, function (_, id) { return '@' + (userName(id, gid) || '알 수 없음'); })
      .replace(/<@&(\d+)>/g, function (_, id) {
        var r = safe(function () { return stores.GuildRoleStore.getRole(gid, id); }, null) ||
          safe(function () { return stores.GuildStore.getGuild(gid).roles[id]; }, null);
        return '@' + (r && r.name ? r.name : '역할');
      })
      .replace(/<#(\d+)>/g, function (_, id) {
        var c = safe(function () { return stores.ChannelStore.getChannel(id); }, null);
        return '#' + (c && c.name ? c.name : '채널');
      })
      .replace(/<a?:(\w+):\d+>/g, ':$1:')
      .replace(/<t:(\d+)(:\w)?>/g, function (_, t) { return new Date(+t * 1000).toLocaleString('ko-KR'); });
  }

  function msgInfo(m, gid) {
    var a = m.author || {};
    var member = gid ? safe(function () { return stores.GuildMemberStore.getMember(gid, a.id); }, null) : null;
    var ref = m.messageReference;
    var refMsg = ref && ref.message_id ? safe(function () { return stores.MessageStore.getMessage(ref.channel_id, ref.message_id); }, null) : null;
    var files = [];
    (m.attachments || []).forEach(function (f) {
      files.push({ url: String(f.proxy_url || f.url || ''), name: String(f.filename || ''), type: String(f.content_type || ''),
        w: f.width | 0, h: f.height | 0 });
    });
    (m.embeds || []).forEach(function (e) {
      var img = e.image || e.thumbnail;
      if (img && (img.proxyURL || img.url)) {
        files.push({ url: String(img.proxyURL || img.url), name: String(e.title || ''), type: 'image/embed', w: img.width | 0, h: img.height | 0 });
      }
    });
    return {
      id: String(m.id),
      author: String(a.id || ''),
      name: (member && member.nick) ? String(member.nick) : String(a.globalName || a.global_name || a.username || ''),
      avatar: a.avatar ? String(a.avatar) : '',
      color: member && member.colorString ? String(member.colorString) : '',
      bot: !!a.bot,
      time: safe(function () { return +new Date(m.timestamp); }, 0) || 0,
      edited: !!m.editedTimestamp,
      text: plain(m.content, gid),
      type: m.type | 0,
      state: String(m.state || ''),
      files: files,
      reply: refMsg ? { name: userName(refMsg.author && refMsg.author.id, gid), text: plain(refMsg.content, gid).slice(0, 120) } : null,
      reactions: (m.reactions || []).map(function (r) {
        return { emoji: String((r.emoji && r.emoji.name) || ''), count: r.count | 0, me: !!r.me };
      }),
    };
  }

  function messagesOf(cid) {
    var ms = safe(function () { return stores.MessageStore.getMessages(cid); }, null);
    var arr = ms ? (safe(function () { return ms.toArray(); }, null) || ms._array || []) : [];
    var ch = safe(function () { return stores.ChannelStore.getChannel(cid); }, null);
    var gid = ch && ch.guild_id ? String(ch.guild_id) : null;
    var out = [];
    var start = Math.max(0, arr.length - 150);
    for (var i = start; i < arr.length; i++) {
      var info = safe(function () { return msgInfo(arr[i], gid); }, null);
      if (info) out.push(info);
    }
    return {
      channel: String(cid),
      title: ch ? String(ch.name || '') : '',
      topic: ch && ch.topic ? String(ch.topic) : '',
      dm: !gid,
      hasMore: !!(ms && ms.hasMoreBefore),
      loading: !!(ms && ms.loadingMore),
      messages: out,
    };
  }

  function guildList() {
    var G = stores.GuildStore, S = stores.SortedGuildStore, R = stores.GuildReadStateStore;
    var ids = safe(function () { return S.getFlattenedGuildIds(); }, null);
    if (!ids) ids = safe(function () { return Object.keys(G.getGuilds()); }, []);
    var out = [];
    for (var i = 0; i < ids.length; i++) {
      var id = ids[i];
      var g = safe(function () { return G.getGuild(id); }, null);
      if (!g) continue;
      out.push({
        id: String(g.id), name: String(g.name || ''), icon: g.icon ? String(g.icon) : '',
        unread: !!safe(function () { return R.hasUnread(id); }, false),
        mentions: safe(function () { return R.getMentionCount(id); }, 0) || 0,
      });
    }
    return out;
  }

  function channelInfo(c) {
    var RS = stores.ReadStateStore;
    return {
      id: String(c.id), name: String(c.name || ''), type: c.type | 0,
      parent: c.parent_id ? String(c.parent_id) : '', pos: c.position | 0,
      unread: !!safe(function () { return RS.hasUnread(c.id); }, false),
      mentions: safe(function () { return RS.getMentionCount(c.id); }, 0) || 0,
      avatar: '', user: '',
    };
  }

  function channelsOf(gid) {
    var out = [];
    if (!gid || gid === '@me') {
      var U = stores.UserStore;
      var pcs = safe(function () { return stores.ChannelStore.getSortedPrivateChannels(); }, null);
      if (!pcs) pcs = safe(function () { return Object.values(stores.ChannelStore.getMutablePrivateChannels()); }, []);
      for (var i = 0; i < pcs.length && out.length < 100; i++) {
        var c = pcs[i];
        var info = channelInfo(c);
        var rid = safe(function () { return c.getRecipientId(); }, null) ||
          safe(function () { return (c.recipients || [])[0]; }, null);
        var u = rid ? safe(function () { return U.getUser(rid); }, null) : null;
        if (!info.name) info.name = u ? String(u.globalName || u.username || '') : '';
        if (!info.name) info.name = 'DM';
        if (u) { info.user = String(u.id); info.avatar = u.avatar ? String(u.avatar) : ''; }
        out.push(info);
      }
      return out;
    }
    var all = safe(function () { return stores.GuildChannelStore.getChannels(gid); }, null);
    if (!all) return out;
    var seen = {};
    [all.SELECTABLE, all.VOCAL, all[4], all['4']].forEach(function (list) {
      (list || []).forEach(function (e) {
        var c = e && (e.channel || e);
        if (!c || !c.id || seen[c.id]) return;
        seen[c.id] = 1;
        out.push(channelInfo(c));
      });
    });
    return out;
  }

  function snapshot() {
    var gid = safe(function () { return stores.SelectedGuildStore.getGuildId(); }, null) || '@me';
    var cid = String(safe(function () { return stores.SelectedChannelStore.getChannelId(); }, '') || '');
    var ch = cid ? safe(function () { return stores.ChannelStore.getChannel(cid); }, null) : null;
    var title = '';
    if (ch) {
      title = String(ch.name || '');
      if (!title) {
        var rid = safe(function () { return ch.getRecipientId(); }, null);
        title = rid ? userName(rid, null) : '';
      }
    }
    return {
      ready: true,
      me: me(),
      title: title,
      voice: !!(ch && (ch.type === 2 || ch.type === 13)),
      guilds: guildList(),
      guild: String(gid),
      channel: String(safe(function () { return stores.SelectedChannelStore.getChannelId(); }, '') || ''),
      channels: channelsOf(gid),
    };
  }

  var last = '';
  var timer = null;
  function send() {
    timer = null;
    if (!ready || !B) return;
    var json = safe(function () { return JSON.stringify(snapshot()); }, '');
    if (json && json !== last) {
      last = json;
      try { B.state(json); } catch (e) {}
    }
  }
  function schedule() { if (!timer) timer = setTimeout(send, 250); }

  var lastMsgs = '';
  var msgTimer = null;
  function sendMessages() {
    msgTimer = null;
    if (!ready || !B || typeof B.messages !== 'function') return;
    var cid = safe(function () { return stores.SelectedChannelStore.getChannelId(); }, null);
    if (!cid) return;
    var json = safe(function () { return JSON.stringify(messagesOf(cid)); }, '');
    if (json && json !== lastMsgs) {
      lastMsgs = json;
      try { B.messages(json); } catch (e) {}
    }
  }
  function scheduleMessages() { if (!msgTimer) msgTimer = setTimeout(sendMessages, 120); }

  window.__lumenMessages = function (cid) { return ready ? JSON.stringify(messagesOf(cid)) : '{}'; };

  window.__lumenSend = function (cid, text) {
    var a = findActions();
    if (!a) return 'no-actions';
    try {
      a.sendMessage(cid, { content: String(text), tts: false, invalidEmojis: [], validNonShortcutEmojis: [] }, undefined, {});
      return 'ok';
    } catch (e) { return 'error: ' + e.message; }
  };

  window.__lumenOlder = function (cid) {
    var a = findActions();
    var ms = safe(function () { return stores.MessageStore.getMessages(cid); }, null);
    var first = ms ? (safe(function () { return ms.first(); }, null) || (ms._array || [])[0]) : null;
    if (!a || !first) return 'none';
    try { a.fetchMessages({ channelId: cid, before: first.id, limit: 50 }); return 'ok'; } catch (e) { return 'error: ' + e.message; }
  };

  window.__lumenChannels = function (gid) {
    if (!ready) return '[]';
    return JSON.stringify(channelsOf(gid));
  };

  window.__lumenNav = function (path) {
    var t = findTransition();
    if (t) { try { t(path); return 'router'; } catch (e) {} }
    try {
      history.pushState(null, '', path);
      window.dispatchEvent(new PopStateEvent('popstate', { state: null }));
    } catch (e) {}
    // 라우터가 못 알아들었으면 그 주소로 새로 연다.
    var want = path.split('/').pop();
    setTimeout(function () {
      var now = safe(function () { return stores.SelectedChannelStore.getChannelId(); }, null);
      if (want && want !== '@me' && now !== want) location.assign(path);
    }, 800);
    return 'history';
  };

  // "PC 앱을 사용하세요" 같은 다운로드 권유 띠를 걷는다.
  function hideNags() {
    var nodes = document.querySelectorAll('[class*="notice_"]');
    for (var i = 0; i < nodes.length; i++) {
      var n = nodes[i];
      if (n.style.display === 'none') continue;
      var t = n.textContent || '';
      if (/다운로드|Download|PC 앱|desktop app/i.test(t)) n.style.display = 'none';
    }
  }

  var tries = 0;
  var boot = setInterval(function () {
    tries++;
    hideNags();
    if (!ready && scanStores()) {
      ready = true;
      Object.keys(stores).forEach(function (n) {
        if (/^(Guild|SortedGuild|Channel|GuildChannel|SelectedGuild|SelectedChannel|ReadState|GuildReadState|User)Store$/.test(n)) {
          try { stores[n].addChangeListener(schedule); } catch (e) {}
        }
        if (/^(Message|SelectedChannel|GuildMember)Store$/.test(n)) {
          try { stores[n].addChangeListener(scheduleMessages); } catch (e) {}
        }
      });
      send();
    }
    if (ready) { schedule(); scheduleMessages(); }
    if (tries > 600) clearInterval(boot);
  }, ready ? 3000 : 1000);
})();
