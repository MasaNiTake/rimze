# RIMZE プロジェクト設計レビュー

本ドキュメントは、コードベース全体と設計ドキュメント（[`PLAN.md`](PLAN.md), [`Agents.md`](Agents.md)）を照合して抽出した「良くない設計」を整理したものです。
ユーザーの指示により「使っていない関数によるエラー」は除外し、アーキテクチャ・設計・パフォーマンス・要件乖離に焦点を当てています。

---

## 🔴 重大な設計問題（優先度：高）

### 1. PDF サポートが「宣言だけ」で実装が存在しない
- [`Cargo.toml`](Cargo.toml:28): `lopdf = "0.30.0"` を依存に追加
- [`src/content.rs`](src/content.rs:117): `FileType::Pdf(PdfFile)` を定義し、[`PdfFile`](src/content.rs:179) 構造体も用意
- [`src/content.rs`](src/content.rs:322): `load_comic_file` で `FileType::Pdf` を生成
- **しかし PDF を画像としてレンダリングする処理が一切存在しない**
  - [`src/main.rs:577`](src/main.rs:577) の `load_image_for_display` は `FileType::Image` と `FileType::Zip` しか処理せず、PDF は `_ => return` で黙って無視される（エラーすら出ない）
  - [`src/thumbnail.rs:122`](src/thumbnail.rs:122) のサムネイル生成でも PDF は未対応
- **根本問題**: `lopdf` は PDF の構造解析用ライブラリであり、**画像レンダリング機能を持たない**。PDF を表示するには `pdfium-render`, `mupdf`, `pdf`（rust-pdf）等のレンダリング系ライブラリが必要
- ユーザー視点：PDF を開いても何も起きない（無言の失敗）
- **対応方針**: 今回は保留。他の修正完了後に別タスクで実装

### 2. 「strict LRUキャッシュ」が要件にあるが、実装はウィンドウ方式（要件乖離）
- [`Agents.md`](Agents.md:44): “Implement an strict LRU (Least Recently Used) caching mechanism” と明記
- しかし実装（[`src/content.rs:481`](src/content.rs:481) の `ImageCache`）は LRU ではない：
  - `window: HashSet<CacheKey>` は順序情報を持たないため、LRU の“使われた順序”を追跡できない
  - [`update_window`](src/content.rs:596) は単に“ウィンドウ外のキーを削除”するだけ
  - [`insert`](src/content.rs:542) は「メモリ上限を超えるなら挿入しない」という消極対応であり、**LRU のように古いエントリを追い出して新規エントリを入れる**動作ではない
  - [`set_max_memory_usage`](src/content.rs:626) にも `// TODO: ここでメモリが上限を超えていたら削除処理を走らせる` と未実装コメント
- 結果：メモリ上限に達した後は新規キャッシュ挿入が一切行われず、ページ送り体験が劣化する可能性
- **対応方針**: **最優先で真の LRU を実装**

### 3. ZIP 内ページのプリフェッチが未実装（パフォーマンス要件未達成）
- [`src/main.rs:703`](src/main.rs:703): `update_cache_and_prefetch` 内で `CacheKey::ZipEntry` のプリフェッチが `return;` でスキップされている
- コメント: 「ZIPのプリフェッチは複雑なため、一旦実装を省略します」
- [`PLAN.md`](PLAN.md:101), [`Agents.md`](Agents.md:50): 「両側の読み込み」「Show the first frame/page immediately, pushing background tasks for subsequent pages」が要件
- 結果：ZIP 内のページ送りで毎回キャッシュミスが発生し、UX 要件（スムーズなページ送り）が満たせていない
- **対応方針**: **実装する**

### 4. `rfd::FileDialog::pick_file()` が UI スレッドをブロックする
- [`src/main.rs:286`](src/main.rs:286): `handle_ui_command` 内で同期API `pick_file()` を直接呼出
- このメソッドは [`update`](src/main.rs:109)（egui の UI スレッド）から呼ばれるため、ダイアログが開いている間アプリ全体の描画が停止する
- `rfd` には非同期版 `rfd::AsyncFileDialog` があるので、`tokio_rt.spawn` 経由で実行し、結果を `UiUpdateMsg` で受ける設計に修正すべき
- **対応方針**: **修正する（詳細は後述の深掘り参照）**

### 5. UI↔非同期タスク間通信に `std::sync::mpsc` を使用（設計意図との乖離）
- [`src/main.rs:5`](src/main.rs:5), [`src/main.rs:204`](src/main.rs:204): `std::sync::mpsc` を使用
- [`PLAN.md`](PLAN.md:102), [`Agents.md`](Agents.md:49): `tokio::sync::mpsc::channel` を使うことが明記されている
- `std::sync::mpsc::Sender::send` はブロッキング可能性があり、Tokioランタイム上のタスクから送信するのは不適切
- また [`update`](src/main.rs:131) の `while let Ok(msg) = self.update_rx.try_recv()` は1フレーム内で処理しきれない場合、メッセージが滞留し表示遅延の原因になる
- egui の `Context::request_repaint()` と組み合わせて、メッセージ到着時に再描画を促す設計が望ましい
- **対応方針**: **`tokio::sync::mpsc` に修正**

---

## 🟠 中程度の設計問題（優先度：中）

### 6. 大きな画像データ（`Vec<u8>`）の不要なクローンが多発
- [`ImageCache::get`](src/content.rs:522) が `.cloned()` で `Vec<u8>` 全体をコピー
- [`load_image_for_display`](src/main.rs:583): キャッシュから取得した画像データをクローン
- [`try_trigger_thumbnail`](src/main.rs:586): 同じデータをさらにクローンして渡す
- [`load_from_source_and_display`](src/main.rs:632): `data.clone()` でキャッシュ挿入とデコードで2度保持
- 数MBの画像が毎フレームコピーされる可能性があり、`Arc<Vec<u8>>` で保持する等の設計変更が必要
- **対応方針**: **`Arc<Vec<u8>>` で共有化（詳細は後述の深掘り参照）**

### 7. `std::sync::Mutex` を Tokio の非同期タスク内でロックしている
- [`src/main.rs:55`](src/main.rs:55): `image_cache: Arc<Mutex<content::ImageCache>>`（`std::sync::Mutex`）
- 非同期タスク内（[`src/main.rs:583`](src/main.rs:583), [`src/main.rs:632`](src/main.rs:632), [`src/main.rs:693`](src/main.rs:693) 等）でロック取得
- ロック中に処理が走るとワーカースレッドがブロックされ、Tokio のランタイム効率が落ちる
- 短時間ロックなら実害は限定的だが、イディオマティックには `tokio::sync::Mutex` またはロック範囲を極小化すべき
- **対応方針**: **`tokio::sync::Mutex` に修正**

### 8. ログ基盤が4種類混在し、未使用依存が大量にある
- [`Cargo.toml`](Cargo.toml:14): `env_logger`, `log`, `simplelog`, `tracing`, `tracing-subscriber` の5クレート
- 実使用は `tracing` + `tracing-subscriber` のみ（[`src/main.rs:4`](src/main.rs:4), [`src/main.rs:20`](src/main.rs:20)）
- `log`, `env_logger`, `simplelog` は未使用依存。ビルド時間・バイナリサイズの無駄
- **対応方針**: **整理＋RAM使用量ロギング追加**

### 9. `async_zip` と `zip` の二重依存（`async_zip` が未使用）
- [`Cargo.toml`](Cargo.toml:9): `async_zip = { version = "*", features = ["full"] }`
- [`Cargo.toml`](Cargo.toml:27): `zip = "0.6.6"`
- 実コードは [`src/content.rs:7`](src/content.rs:7), [`src/thumbnail.rs:135`](src/thumbnail.rs:135) で `zip::ZipArchive` のみ使用
- [`PLAN.md`](PLAN.md:36): `async_zip` を使う計画だったが、実装は同期 `zip` を `spawn_blocking` で包む形に妥協
- **対応方針**: **`async_zip` を削除**

### 10. `unwrap()` / `expect()` の多用（コーディング規約違反）
- [`Agents.md`](Agents.md:14): “Minimize the use of `unwrap()` or `expect()`” と明記
- 主な箇所:
  - [`src/main.rs:238`](src/main.rs:238), [`src/main.rs:239`](src/main.rs:239): `fonts.families.get_mut(...).unwrap()`
  - [`src/main.rs:252`](src/main.rs:252): `runtime::Builder::...build().unwrap()`
  - `image_cache.lock().unwrap()` が多数（[`src/main.rs:322`](src/main.rs:322), [`src/main.rs:427`](src/main.rs:427), [`src/main.rs:583`](src/main.rs:583) 等）
- 特に `Mutex::lock().unwrap()` は poison ロック時に全滅するため、エラーハンドリングすべき
- **対応方針**: **最低限の使用に削減**

### 11. 画像デコード処理（`image::load_from_memory` → RGBA8 → `ColorImage`）が3箇所で重複（DRY 違反）
- [`ImageFile::get_egui_color_image`](src/content.rs:148)
- [`decode_and_display`](src/main.rs:665)
- [`load_from_source_and_display`](src/main.rs:633)
- [`Agents.md`](Agents.md:7) の DRY 原則に違反。共通ヘルパに抽出すべき
- **対応方針**: **共通ヘルパで DRY 化**

### 12. `Directory` 構造体の役割が曖昧で、状態が重複管理されている
- [`MyApp`](src/main.rs:50) が `directory`, `parent_directory`, `current_directory_path` を別々に保持
- `directory.path` と `current_directory_path` は同じもののはずだが、別々に更新されるため不整合リスク
- さらに [`FileType::Directory`](src/content.rs:119) としても利用（[`load_comic_file`](src/content.rs:295) では `files: vec![]` の空ディレクトリとして生成）
- 単一のソース・オブ・トゥルースに統一すべき
- **対応方針**: **現状維持**（ユーザー理解済み：画像パスとディレクトリで差が出る可能性あり）

### 13. `Cargo.toml` のバージョン指定に `*` を多用（再現性リスク）
- [`Cargo.toml`](Cargo.toml:9): `async_zip`, `eframe`, `egui_extras`, `serde_yaml`, `tokio`, `tokio-util` が `*` 指定
- `Cargo.lock` で固定はされるが、`cargo update` で破壊的変更が混入するリスク
- [`GEMINI.md`](GEMINI.md:5) に記載の eframe v0.34 トラブルはまさにこの事例
- キャレット `^` でメジャーバージョンを固定すべき
- **対応方針**: **今は放置**（仮対応段階のため）

### 14. `JoinHandle` が無視され、タスクpanicを検知できない
- [`load_from_source_and_display`](src/main.rs:618), [`try_trigger_thumbnail`](src/main.rs:606), [`update_cache_and_prefetch`](src/main.rs:700), [`load_and_open_path`](src/main.rs:442), [`load_directory_content`](src/main.rs:544), [`ui_file_drag_and_drop`](src/main.rs:376) で `tokio_rt.spawn(...)` の戻り値を捨てている
- `JoinError` をハンドリングしないと、タスク内のpanicがサイレントになる
- **対応方針**: **panic を拾う**

---

## 🟡 軽微な問題（優先度：低）

### 15. `SortOrder` のコメントが実装と矛盾している
- [`src/content.rs:211`](src/content.rs:211): `Ascending` を「新しいもの」と説明しているが、日時昇順なら「古いもの」が先が直感
- 実装（[`src/content.rs:446`](src/content.rs:446)）は通常の `cmp` なので昇順＝古いもの先。コメントが誤解を招く
- **対応方針**: **コメント修正**

### 16. 英語コメントが残存（コーディング規約違反）
- [`src/main.rs:103`](src/main.rs:103): `// eframe 0.34+ requires the ui method...` が英語
- [`src/main.rs:105`](src/main.rs:105): `// However, we completely override...` も英語
- [`Agents.md`](Agents.md:23): コメントは日本語と明記されている
- **対応方針**: **日本語化**

### 17. `ThumbnailWorker::stop()` がアプリ終了時に呼ばれない
- [`ThumbnailWorker`](src/thumbnail.rs:16) に `stop()` メソッドはあるが、`MyApp` に `Drop` 実装がない
- `Arc` の循環参照はないため実害は限定的だが、明示的クリーンアップが欠落
- **対応方針**: **`Drop` 実装で呼ぶ**

### 18. `max_load_use_memory` のスライダー上限がハードコード
- [`src/view.rs:121`](src/view.rs:121): `Slider::new(&mut max_mem_mb, 10..=1000)` で 10–1000MB に固定
- 高解像度画像を多数キャッシュしたいユーザーにとって 1GB 上限は小さい
- 設定可能範囲を拡大、または `AppSettings` に移動すべき
- **対応方針**: **`AppSettings` に移動**

---

## 🔍 ユーザー要請による深掘り説明

### #4 の深掘り: `rfd::FileDialog` の UI スレッドブロック

egui の [`update()`](src/main.rs:109) は UI スレッド（メインスレッド）で実行されます。この中から [`handle_ui_command`](src/main.rs:286) 経由で [`rfd::FileDialog::pick_file()`](src/main.rs:287) を呼ぶと、**ネイティブダイアログがモーダル表示され、ユーザーがファイルを選ぶまでこの関数から戻りません**。

具体的な影響:
1. **ダイアログ表示中、`update()` ループが停止** — egui の再描画が止まり、ウィンドウがフリーズや白画面に見える
2. **バックグラウンドタスクの結果が反映されない** — ダイアログ中は [`try_recv`](src/main.rs:131) ループが回らないため、プリフェッチ完了画像の表示が遅延する
3. **OS の「応答なし」判定リスク** — 長時間のダイアログ操作で OS から強制終了対象と判定される可能性

修正方法（`rfd::AsyncFileDialog` への移行）:
- [`rfd::AsyncFileDialog`](https://docs.rs/rfd/latest/rfd/struct.AsyncFileDialog.html) は `Future<Output = Option<FileHandle>>` を返す非同期API
- [`tokio_rt.spawn`](src/main.rs:442) 内で `.await` し、結果を [`UiUpdateMsg`](src/main.rs:83) の新バリアント（例: `FilePicked(PathBuf)`）経由で UI に返す
- UI スレッドはブロックされず、ダイアログ中も描画・プリフェッチが継続
- 注意: `AsyncFileDialog` は内部的にプラットフォーム別スレッドを使うため、`tokio` のマルチスレッドランタイムと相性が良い

### #6 の深掘り: `Vec<u8>` のクローン問題と現在のデータフロー

ユーザー認識「ZIP/生画像を `Vec<u8>` として RAM にコピーし、egui 用に RGBA に変換している」は **正しい** です。現在のフローは:

```
[ZIP/ファイル] → 生バイト Vec<u8> → キャッシュ(RAM) → デコード → RGBA8 ColorImage → egui Texture
```

問題は **「キャッシュから取り出す際のコピー」** であり、「RAM への保持」自体ではありません:

- [`ImageCache::get`](src/content.rs:522): `cache.get(key).cloned()` で `Vec<u8>` 全体をコピー
- [`load_image_for_display`](src/main.rs:585): 取得したコピーを [`decode_and_display`](src/main.rs:665) と [`try_trigger_thumbnail`](src/main.rs:596) の **2箇所に消費**

`image::load_from_memory(&data)` は `&[u8]` を取るため、**データを消費（move）する必要はありません**。つまり毎回のフルコピーは不要です。

修正方針（`Arc<Vec<u8>>` で共有）:
- `ImageCache` の値型を `Vec<u8>` → `Arc<Vec<u8>>` に変更
- `get()` は `Arc<Vec<u8>>` を返す（クローンは参照カウント増加分のみ、数バイト）
- デコード・サムネイル関数は `&[u8]` を取るように統一
- 効果: 画像1枚5MBとすると、ページ送り1回あたり 5MB のアロケーションが消滅

---

## 📋 ユーザー承認済みの対応方針（2026-07-19）

| # | 問題 | 方針 |
|---|------|------|
| 1 | PDF サポート未実装 | **後日実装**（他の修正が終わってから） |
| 2 | LRU キャッシュ未達成 | **最優先で実装** |
| 3 | ZIP プリフェッチ未実装 | **実装する** |
| 4 | rfd の UI ブロック | **修正する**（AsyncFileDialog 化） |
| 5 | `std::sync::mpsc` 使用 | **`tokio::sync::mpsc` に修正** |
| 6 | `Vec<u8>` の不要クローン | **`Arc<Vec<u8>>` で共有化** |
| 7 | `std::sync::Mutex` 使用 | **`tokio::sync::Mutex` に修正** |
| 8 | ログ基盤混在 | **整理＋RAM使用量ロギング追加** |
| 9 | `async_zip` 未使用 | **削除** |
| 10 | `unwrap`/`expect` 多用 | **最低限に削減** |
| 11 | デコード処理の3重複 | **共通ヘルパで DRY 化** |
| 12 | `Directory` 状態重複 | **現状維持**（ユーザー理解済み） |
| 13 | `Cargo.toml` の `*` | **今は放置**（仮対応） |
| 14 | `JoinHandle` 無視 | **panic を拾う** |
| 15 | `SortOrder` コメント矛盾 | **コメント修正** |
| 16 | 英語コメント残存 | **日本語化** |
| 17 | `ThumbnailWorker::stop()` 未呼出 | **`Drop` 実装で呼ぶ** |
| 18 | スライダー上限ハードコード | **`AppSettings` に移動** |
