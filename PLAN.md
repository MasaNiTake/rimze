# 漫画ビューア (RIMZE) 開発計画と手動テスト項目

このドキュメントは人間（開発者）向けの開発計画と、AI（自動テスト）ではカバーできない手動テストの項目をまとめたものです。

## 手動テスト項目 (Manual Testing)

AIによる自動テスト（`cargo test`等）に加えて、以下の項目はUIや描画に直接関わるため、人間による目視および操作テストが必要です。

- [ ] **UIレイアウトの崩れ確認**: 
    - ウィンドウをリサイズした際に、左側のファイルリスト、中央の画像、下部のスライドバーが正しく追従・リサイズされるか。
    - 画像がアスペクト比を維持したまま、ウィンドウ内に最大化されて表示されているか。
- [ ] **ファイル操作とソートの動作確認**: 
    - プルダウンメニューからソート順（ファイル名、更新日時、作成日時）を変更した際、ファイルリストの順序が正しく反映されるか。
    - 異なるフォーマット（ZIP, PDF, 各種画像）を選択した際に、正しくデコードされて中央に表示されるか。
- [ ] **ページ送りとスライドバー操作**: 
    - 下部のスライドバーを操作して、意図したページに切り替わるか。
    - ZIPやPDF内の画像キャッシュが機能し、ページ送りがスムーズに行えるか（カクつきがないか）。
- [ ] **メモリ管理・解放の確認**: 
    - 大量の画像を含むディレクトリや重いZIPファイルを連続で開いた際、アプリがクラッシュせずに設定したメモリ上限内で動作し続けるか。
    - 新しい画像へ遷移した際、古い画像（LRU）のメモリが解放されているか（OSのリソースモニター等で確認）。

---

## 開発計画の詳細

### 計画の概要:

1.  **プロジェクト構造の確認と初期設定**: 既存のRustプロジェクトの構造を確認し、必要な依存関係を`Cargo.toml`に追加します。
2.  **UIコンポーネントの設計**: Eguiを使用して、左側のファイルリスト、プルダウンメニュー、画像表示領域、下部のスライドバーを実装します。
3.  **ファイルシステム操作とソート**:
    *   指定されたディレクトリ内の漫画ファイルをリストアップし、ユーザーが選択したソート順で並べ替える機能を実装します。
    *   `tokio::fs`などの非同期ファイルI/Oを使用します。
4.  **漫画ファイルの読み込みとデコード**:
    *   ZIP、PDF、画像ファイル（webp, png, jpg）の読み込みとデコードを非同期で行います。
    *   Tokioと互換性のあるクレートを選定します。
        *   ZIP: `async_zip`
        *   PDF: `lopdf` (ただし、非同期対応は要確認、必要に応じてラッパーを検討)
        *   画像: `image`クレート (非同期対応は要確認、必要に応じてラッパーを検討)
    *   デコードされた画像をEguiで表示可能な形式に変換します。
5.  **メモリ管理とキャッシュ**:
    *   ユーザーが指定したメモリ使用量に基づいて、画像のキャッシュを管理します。
    *   LRU (Least Recently Used) キャッシュのような仕組みを検討し、メモリ上限に達した場合に最も古い画像を解放するロジックを実装します。
    *   特に、更新日時順ソートで新しい画像に移動した場合のメモリ解放ロジックを実装します。
6.  **画像表示ロジック**:
    *   画像表示領域で、画像を中央に、可能な限り大きく表示するロジックを実装します。
    *   下部のスライドバー領域を考慮に入れます。
    *   最初の1枚が読み込まれたらすぐに表示し、残りの画像をバックグラウンドで読み込むようにします。
7.  **プルダウンメニューと設定**:
    *   ファイル操作（開く、閉じるなど）や設定（ソート順、メモリ上限など）のためのプルダウンメニューを実装します。

### フェーズ1: プロジェクトのセットアップとUIの骨格

1.  **`Cargo.toml`の更新**:
    *   `tokio` (full features), `egui`, `eframe` を追加。
    *   画像処理 (`image`, `webp`), ZIP (`async_zip`), PDF (`lopdf`) 関連のクレートを追加。
    *   非同期処理を考慮し、`tokio`のランタイム設定を検討。
    *   `directories`クレートで設定ファイルのパスを管理する可能性も検討。
2.  **`src/main.rs`の初期設定**:
    *   Eguiアプリケーションの基本的な構造をセットアップ。
    *   `eframe::run_native`を使用してウィンドウを作成。
3.  **`src/view.rs`のUIレイアウト**:
    *   Eguiの`CentralPanel`, `SidePanel`, `TopBottomPanel`を使用して、左側のファイルリスト、上部のメニュー、中央の画像表示領域、下部のスライドバーの基本的なレイアウトを定義。
    *   ダミーのファイルリストと画像表示を配置。

### フェーズ2: ファイルシステム操作とソート

1.  **`src/content.rs`のファイルリスト管理**:
    *   `ComicFile`構造体を定義し、パス、種類（ZIP, PDF, Image, Directory）、最終更新日時、作成日時などのメタデータを保持。
    *   指定されたディレクトリを非同期で走査し、`ComicFile`のリストを生成する関数を実装。
    *   ユーザーが選択したソート順（ファイル名、更新日時、作成日時）に基づいて`ComicFile`のリストをソートするロジックを実装。
    *   `tokio::fs::read_dir`と`tokio::fs::metadata`を使用。

### フェーズ3: 漫画ファイルの読み込みとデコード

1.  **`src/content.rs`のデコードロジック**:
    *   `ComicLoader`のような構造体を定義し、ZIP、PDF、画像ファイルのデコードを担当。
    *   **ZIP**: `async_zip`クレートを使用して、ZIPファイル内の画像を非同期で読み込み、デコード。
    *   **PDF**: `lopdf`クレートを使用してPDFを読み込み、各ページを画像としてレンダリング。`lopdf`は同期的なので、`tokio::task::spawn_blocking`を使って非同期コンテキストで実行することを検討。
    *   **画像**: `image`クレートを使用して、webp, png, jpgをデコード。これも同期的なので、`spawn_blocking`を検討。
    *   デコードされた画像データをEguiのテクスチャとして扱える形式（例: `egui::ColorImage`）に変換。
    *   `tokio::sync::mpsc::channel`などを使って、読み込み完了した画像をUIスレッドに送る仕組みを検討。

### フェーズ4: メモリ管理とキャッシュ

1.  **`src/content.rs`のキャッシュ管理**:
    *   `ImageCache`のような構造体を定義し、`HashMap`と`VecDeque`（または`linked_hash_map`のようなLRUキャッシュクレート）を組み合わせて実装。
    *   キャッシュに画像を保存する際に、現在のメモリ使用量を追跡。
    *   メモリ上限（ユーザー設定）に達した場合、最も古い（LRU）画像をキャッシュから削除するロジックを実装。
    *   特に、更新日時順ソートで新しい画像に移動した場合、古い画像を積極的に解放するロジックを実装。
    *   `Arc<Mutex<ImageCache>>`などで複数のスレッドから安全にアクセスできるようにする。

### フェーズ5: UIとロジックの連携

1.  **`src/view.rs`のUI更新ロジック**:
    *   左側のファイルリストに`ComicFile`を表示し、選択されたファイルに応じて中央の画像表示を更新。
    *   画像表示領域で、デコードされた画像を中央に、アスペクト比を保ちつつ最大サイズで表示。下部のスライドバー領域を避けるように調整。
    *   下部のスライドバーを実装し、画像のページ送りを制御。
    *   プルダウンメニューからソート順やメモリ上限を設定できるようにする。
2.  **非同期処理の統合**:
    *   EguiのUIスレッドとは別に、Tokioランタイム上でファイル読み込みとデコードを行うタスクを起動。
    *   `tokio::spawn`を使用して、バックグラウンドで「両側の読み込み」を実行。
    *   読み込み完了した画像はチャネルを通じてUIスレッドに送られ、Eguiのテクスチャとして登録・表示。

### フェーズ6: エラーハンドリングと設定保存

1.  **エラーハンドリング**:
    *   ファイルI/O、デコード、メモリ管理におけるエラーを適切に処理し、ユーザーにフィードバックを提供。
2.  **設定保存**:
    *   ユーザーが設定したソート順やメモリ上限などの設定を、`confy`や`serde`と組み合わせてファイルに保存・読み込みする機能を検討。

## Mermaid Diagram (高レベルのアーキテクチャ)

```mermaid
graph TD
    A[Egui UI] --> B{Application State};
    B --> C[File List Panel];
    B --> D[Image Display Panel];
    B --> E[Top Menu / Settings];
    B --> F[Bottom Slider];

    B -- Requests --> G[Comic Loader];
    G -- Reads --> H[File System (ZIP, PDF, Images)];
    G -- Decodes --> I[Image Data];
    I -- Caches --> J[Image Cache];
    J -- Provides --> D;

    subgraph Async Operations
        G -- Spawns --> K[Tokio Runtime];
        H -- Async I/O --> K;
        I -- Blocking Decoders --> K;
    end

    E -- Configures --> B;
    F -- Navigates --> B;
    C -- Selects --> B;
```

## Mermaid Diagram (データフローとメモリ管理)

```mermaid
graph TD
    A[User Action (Open File/Navigate)] --> B{Application State};
    B -- Triggers Load --> C[Comic Loader];

    C -- Reads File Metadata --> D[File System];
    D -- Provides File List --> B;

    C -- Reads Raw Data (Async) --> E[Tokio Runtime];
    E -- Spawns Blocking Task --> F[Image Decoder (image, lopdf, async_zip)];
    F -- Decoded Image --> G[Image Cache];

    G -- Checks Memory Usage --> H{Memory Manager};
    H -- If Over Limit --> I[Evict Oldest Image];
    I -- Frees Memory --> G;

    G -- Provides Image for Display --> J[Egui Texture];
    J -- Renders --> K[Image Display Panel];

    subgraph Cache Management
        G -- Stores --> L[Image Data (egui::ColorImage)];
        G -- Tracks --> M[LRU List];
        H -- Monitors --> L;
    end
```
