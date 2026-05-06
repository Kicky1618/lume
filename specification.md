# Lume UI Language 仕様書 v0.1

この仕様書は章ごとに分割しました。各ファイルは元の節番号を保っています。

- [0-7. 基礎](specification/00-07-foundations.md)
  - 概要・設計目標・字句・キーワード・基本構造・モジュール・型システム
- [8-18. 言語コア](specification/08-18-language-core.md)
  - 値・式・コンポーネント・状態・view・条件分岐・繰り返し・イベント
- [19-24. UI とルーティング](specification/19-24-ui-composition.md)
  - スタイル・テーマ・標準コンポーネント・アクセシビリティ・ルーター
- [25-28. データ取得と FFI](specification/25-28-data-and-ffi.md)
  - データ取得・Server Actions・C / C++ FFI・フォーム
- [29-32. ターゲットと実行モデル](specification/29-32-targets-runtime.md)
  - コンパイルターゲット・DOM モデル・バックエンド実行モデル・最適化プロファイル
- [33-34. 内部 IR と最適化](specification/33-34-optimization-internals.md)
  - 内部 IR・最適化・コード生成詳細
- [35. 標準モジュール](specification/35-std-modules.md)
  - `lume/std/...` 名前空間、標準コンポーネント、router / FFI / GPU 関連
- [36-40. ツールと設定](specification/35-40-tooling-reference.md)
  - CLI・設定ファイル・Rust API・エラー診断・フォーマット規則
- [41-43. 構文と AST](specification/41-43-grammar-ast.md)
  - EBNF・AST・メモリモデル
- [44-48. ポリシーと配布](specification/44-48-policy-packaging.md)
  - セキュリティ・国際化・アニメーション・テスト・パッケージ仕様
- [49-54. コンパイラと MVP](specification/49-54-compiler-mvp.md)
  - コンパイラ構成・MVP・実例・バージョニング・将来拡張・まとめ
