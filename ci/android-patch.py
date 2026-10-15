#!/usr/bin/env python3
"""`tauri android init`이 만든 안드로이드 프로젝트를 소리에 맞게 고친다.

- 음악 폴더를 읽을 권한(READ_MEDIA_AUDIO, 표지 그림용 READ_MEDIA_IMAGES)을 매니페스트에 넣고,
  처음 켤 때 묻는다.
- 창은 앱 안에서 뜬 소리 서버(http://127.0.0.1:4380)를 연다. 릴리스 빌드에서도 이 평문
  주소를 열 수 있게 한다(밖으로 나가는 주소가 아니다).
- 안드로이드 15부터는 화면이 상태 표시줄·제스처 막대 밑까지 그려진다(edge-to-edge). 화면이
  그 밑에 깔리지 않게 시스템 막대와 키보드 자리만큼 띄우고, 막대 자리는 앱의 어두운 바탕으로 둔다.
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
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {{
  override fun onCreate(savedInstanceState: Bundle?) {{
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // 상태 표시줄·제스처 막대·키보드 자리만큼 화면을 띄운다. 그 자리는 앱의 바탕색, 아이콘은 밝게.
    window.decorView.setBackgroundColor(Color.parseColor("#0E0E10"))
    WindowCompat.getInsetsController(window, window.decorView).apply {{
      isAppearanceLightStatusBars = false
      isAppearanceLightNavigationBars = false
    }}
    val content = findViewById<android.view.View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) {{ v, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      v.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
      WindowInsetsCompat.CONSUMED
    }}
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

# 테마: 처음 뜨는 순간(스플래시)부터 앱의 어두운 바탕
colors = root / 'app/src/main/res/values/colors.xml'
if colors.exists():
    c = colors.read_text()
    if 'sori_bg' not in c:
        c = c.replace('</resources>', '    <color name="sori_bg">#0E0E10</color>\n</resources>')
else:
    c = '<?xml version="1.0" encoding="utf-8"?>\n<resources>\n    <color name="sori_bg">#0E0E10</color>\n</resources>\n'
colors.write_text(c)
for t in root.glob('app/src/main/res/values*/themes.xml'):
    x = t.read_text()
    if 'sori_bg' not in x:
        x = x.replace('<!-- Customize your theme here. -->', '''<item name="android:windowBackground">@color/sori_bg</item>
        <item name="android:statusBarColor">@color/sori_bg</item>
        <item name="android:navigationBarColor">@color/sori_bg</item>
        <item name="android:windowLightStatusBar">false</item>
        <item name="android:windowLightNavigationBar" tools:targetApi="27">false</item>''')
        t.write_text(x)

print('patched', manifest, gradle, act, sep='\n  ')
print(manifest.read_text())
