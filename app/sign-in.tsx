'use client';

import { useState } from 'react';

import type { AccountState } from '../lib/use-account';

/**
 * 로그인하는 자리.
 *
 * 로비와 설정 두 곳에 같은 것이 필요하다. 처음 켠 사람은 로비에서 마주치고, 나중에
 * 생각나서 찾는 사람은 설정에서 찾는다. 한쪽에만 놓아 두면 다른 쪽에서는 "여기서는
 * 안 되나" 하고 화면을 뒤지게 된다.
 *
 * 길이 둘인 이유는 앱과 웹이 다르기 때문이다.
 *
 * 구글은 임베디드 WebView에서 오는 OAuth를 거절하므로 앱에서는 네이티브 창이 떠야 하고,
 * 그 창은 APK 안에 들어 있는 설정을 읽는다 — 즉 APK를 새로 깔기 전에는 켜지지 않는다.
 * 이메일 쪽은 전부 자바스크립트라서 조용한 웹 업데이트만으로 그 자리에서 동작한다.
 * 그래서 지금 깔려 있는 앱에서 클라우드 저장을 켤 수 있는 길은 대개 아래쪽이다.
 */
export function SignIn({ account }: { account: AccountState }) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');

  const ready = email.includes('@') && password.length >= 6 && !account.signingIn;

  return (
    <>
      <p>로그인하면 기록이 계정에 저장되어 폰을 바꿔도 남습니다.</p>

      <button className="secondary" onClick={() => void account.signIn()} disabled={account.signingIn}>
        {account.signingIn ? '로그인 중…' : 'Google로 로그인'}
      </button>

      <p className="label" style={{ marginTop: '0.8rem' }}>
        또는 이메일로
      </p>

      <div className="editor">
        <label>
          <input
            type="email"
            inputMode="email"
            autoComplete="username"
            placeholder="이메일"
            aria-label="이메일"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
          />
        </label>
        <label>
          <input
            type="password"
            autoComplete="current-password"
            placeholder="비밀번호 (6자 이상)"
            aria-label="비밀번호"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
        </label>
        {/*
          로그인과 가입이 버튼 둘인 이유는 `lib/firebase.ts`의 `signInWithEmail`에 적었다 —
          이메일 열거 보호가 켜져 있으면 없는 계정과 틀린 비밀번호가 같은 오류로 오기
          때문에, 코드가 둘을 구분해서 알아서 가입시킬 수 없다.
        */}
        <div className="row">
          <button
            className="secondary"
            disabled={!ready}
            onClick={() => void account.signInEmail(email, password, false)}
          >
            로그인
          </button>
          <button
            className="secondary"
            disabled={!ready}
            onClick={() => void account.signInEmail(email, password, true)}
          >
            가입
          </button>
        </div>
      </div>

      {account.error && <p className="notice error">{account.error}</p>}
    </>
  );
}
