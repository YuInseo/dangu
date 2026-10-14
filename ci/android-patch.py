#!/usr/bin/env python3
"""`tauri android init`이 만든 안드로이드 프로젝트를 소리에 맞게 고친다.

- 음악 폴더를 읽을 권한(READ_MEDIA_AUDIO, 표지 그림용 READ_MEDIA_IMAGES)을 매니페스트에 넣고,
  처음 켤 때 묻는다.
- 창은 앱 안에서 뜬 소리 서버(http://127.0.0.1:4380)를 연다. 릴리스 빌드에서도 이 평문
  주소를 열 수 있게 한다(밖으로 나가는 주소가 아니다).
"""
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else 'crates/sori-app/gen/android')

manifest = root / 'app/src/main/AndroidManifest.xml'
m = manifest.read_text()
perms = [
    '<uses-permission android:name="android.permission.READ_MEDIA_AUDIO" />',
    '<uses-permission android:name="android.permission.READ_MEDIA_IMAGES" />',
    '<uses-permission android:name="android.permission.READ_EXTERNAL_STORAGE" android:maxSdkVersion="32" />',
    '<uses-permission android:name="android.permission.WAKE_LOCK" />',
]
missing = [p for p in perms if p.split('"')[1] not in m]
if missing:
    m = m.replace('<application', '\n    '.join(missing) + '\n\n    <application', 1)
# 평문 주소(127.0.0.1)를 릴리스에서도 열 수 있게
m = re.sub(r'android:usesCleartextTraffic="[^"]*"', 'android:usesCleartextTraffic="true"', m)
if 'usesCleartextTraffic' not in m:
    m = m.replace('<application', '<application android:usesCleartextTraffic="true"', 1)
manifest.write_text(m)

gradle = root / 'app/build.gradle.kts'
g = gradle.read_text()
g = re.sub(r'manifestPlaceholders\["usesCleartextTraffic"\]\s*=\s*"false"', 'manifestPlaceholders["usesCleartextTraffic"] = "true"', g)
gradle.write_text(g)

acts = list(root.glob('app/src/main/java/**/MainActivity.kt'))
if not acts:
    sys.exit('MainActivity.kt를 찾지 못했습니다')
act = acts[0]
pkg = re.search(r'^package\s+([\w.]+)', act.read_text(), re.M).group(1)
act.write_text(f'''package {pkg}

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle

class MainActivity : TauriActivity() {{
  override fun onCreate(savedInstanceState: Bundle?) {{
    super.onCreate(savedInstanceState)
    // 음악 폴더를 읽으려면 사용자의 허락이 필요하다. 허락하면 소리가 몇 초 안에 다시 스캔한다.
    val wanted = if (Build.VERSION.SDK_INT >= 33)
      arrayOf(Manifest.permission.READ_MEDIA_AUDIO, Manifest.permission.READ_MEDIA_IMAGES)
    else
      arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE)
    val missing = wanted.filter {{ checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED }}
    if (missing.isNotEmpty()) requestPermissions(missing.toTypedArray(), 1)
  }}
}}
''')
print('patched', manifest, gradle, act, sep='\n  ')
print(manifest.read_text())
