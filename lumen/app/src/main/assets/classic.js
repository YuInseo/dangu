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
    return {
      ready: true,
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
      });
      send();
    }
    if (ready) { schedule(); }
    if (tries > 600) clearInterval(boot);
  }, ready ? 3000 : 1000);
})();
