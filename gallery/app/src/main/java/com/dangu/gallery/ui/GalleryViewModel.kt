package com.dangu.gallery.ui

import android.app.Application
import android.content.Context
import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.dangu.gallery.data.Album
import com.dangu.gallery.data.GeoPhoto
import com.dangu.gallery.data.LocationIndex
import com.dangu.gallery.data.MediaItem
import com.dangu.gallery.data.MediaRepository
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

enum class ViewMode { Grid, List }

enum class SortOrder(val label: String) {
    DateDesc("최신순"),
    DateAsc("오래된순"),
    NameAsc("이름순"),
    SizeDesc("큰 파일순"),
}

data class GalleryState(
    val loading: Boolean = true,
    val items: List<MediaItem> = emptyList(),
    val albums: List<Album> = emptyList(),
    val viewMode: ViewMode = ViewMode.Grid,
    val columns: Int = 4,
    val sort: SortOrder = SortOrder.DateDesc,
    /** 좌표가 있는 사진들 (지도 탭) */
    val geo: List<GeoPhoto> = emptyList(),
    /** 위치를 읽는 중이면 (읽은 수, 전체) */
    val geoScan: Pair<Int, Int>? = null,
)

class GalleryViewModel(app: Application) : AndroidViewModel(app) {
    val repository = MediaRepository.get(app)
    private val prefs = app.getSharedPreferences("gallery", Context.MODE_PRIVATE)

    private val _state = MutableStateFlow(
        GalleryState(
            viewMode = runCatching { ViewMode.valueOf(prefs.getString("viewMode", null)!!) }
                .getOrDefault(ViewMode.Grid),
            columns = prefs.getInt("columns", 4),
            sort = runCatching { SortOrder.valueOf(prefs.getString("sort", null)!!) }
                .getOrDefault(SortOrder.DateDesc),
        )
    )
    val state: StateFlow<GalleryState> = _state.asStateFlow()

    private val locations = LocationIndex.get(app)
    private var loadJob: Job? = null
    private var geoJob: Job? = null
    private var observing = false

    // 사진을 찍거나 지우면 MediaStore가 알려 준다. 연달아 오므로 잠깐 모았다가 한 번에 다시 읽는다.
    private val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
        override fun onChange(selfChange: Boolean) {
            reload(debounceMs = 400)
        }
    }

    fun start() {
        if (!observing) {
            observing = true
            getApplication<Application>().contentResolver.registerContentObserver(
                MediaStore.Files.getContentUri(MediaStore.VOLUME_EXTERNAL), true, observer
            )
        }
        reload()
    }

    fun reload(debounceMs: Long = 0) {
        loadJob?.cancel()
        loadJob = viewModelScope.launch {
            if (debounceMs > 0) delay(debounceMs)
            val items = repository.loadAll()
            _state.update {
                it.copy(loading = false, items = items, albums = repository.albums(items))
            }
            scanLocations(items)
        }
    }

    private fun scanLocations(items: List<MediaItem>) {
        geoJob?.cancel()
        geoJob = viewModelScope.launch {
            var lastPush = 0L
            val geo = locations.scan(items) { done, total, partial ->
                // 수백 장마다 지도를 갱신하면 충분하다 — 매번 올리면 표식을 계속 다시 그린다.
                val now = System.currentTimeMillis()
                if (done == 0 || done == total || now - lastPush > 1500) {
                    lastPush = now
                    _state.update { it.copy(geo = partial, geoScan = if (done < total) done to total else null) }
                }
            }
            _state.update { it.copy(geo = geo, geoScan = null) }
        }
    }

    fun setViewMode(mode: ViewMode) {
        prefs.edit().putString("viewMode", mode.name).apply()
        _state.update { it.copy(viewMode = mode) }
    }

    fun setColumns(columns: Int) {
        val c = columns.coerceIn(2, 7)
        prefs.edit().putInt("columns", c).apply()
        _state.update { it.copy(columns = c) }
    }

    fun setSort(sort: SortOrder) {
        prefs.edit().putString("sort", sort.name).apply()
        _state.update { it.copy(sort = sort) }
    }

    override fun onCleared() {
        if (observing) getApplication<Application>().contentResolver.unregisterContentObserver(observer)
    }
}

fun List<MediaItem>.ordered(order: SortOrder): List<MediaItem> = when (order) {
    SortOrder.DateDesc -> sortedByDescending { it.date }
    SortOrder.DateAsc -> sortedBy { it.date }
    SortOrder.NameAsc -> sortedBy { it.name.lowercase() }
    SortOrder.SizeDesc -> sortedByDescending { it.size }
}
