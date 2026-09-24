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

    private var loadJob: Job? = null
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
