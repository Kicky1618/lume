## 0. 概要

Lume UI Language、以下 **Lume** は、Web UI を宣言的に記述し、**HTML + JavaScript + CSS**、または **HTML + JavaScript + CSS + WebAssembly** へ直接コンパイルすることを目的とした UI 構築言語である。

Lume は HTML / CSS / JavaScript を完全に隠すための言語ではない。目的は UI の構造、状態、イベント、スタイル、アクセシビリティ、ルーティング、データ取得を一貫したモデルで記述し、フレームワークを経由せずにブラウザが直接実行できる成果物へ変換することである。

JavaScript の上に記号を撒いただけの新言語ではない、という建前で進める。現実にはコンパイラがすべてを背負う。いつものことだ。

---

## 1. 設計目標

### 1.1 主要目標

Lume は以下を満たすことを目標とする。

1. UI 構造を簡潔かつ明確に記述できること
2. 状態、派生値、イベント処理を分離して扱えること
3. スタイルをトークンベースで管理できること
4. アクセシビリティ違反をコンパイル時検査と runtime 検査の組み合わせで減らせること
5. SSR / CSR で同じ UI 構造を生成できること
6. HTML / JavaScript / CSS / WebAssembly へ直接コンパイルできること
7. ソースマップと診断を提供し、開発体験を犠牲にしないこと
8. 低レベル拡張は C / C++ FFI に限定し、JavaScript 埋め込みを許可しないこと

### 1.2 非目標

以下は Lume の初期仕様では扱わない。

1. 完全な JavaScript 互換実行環境
2. ブラウザ以外の UI を第一級ターゲットにすること
3. React / Vue / Svelte などの既存 UI フレームワークへの依存
4. CSS の全機能を再実装すること
5. 全 UI フレームワークへの完全等価出力
6. ビジュアルエディタ専用言語にすること

---

## 2. ファイル形式

Lume のソースファイル拡張子は `.lume` とする。

```txt
Component.lume
routes.lume
theme.lume
app.lume
```

1 ファイルには複数の宣言を含めることができる。

```lume
import { UserCard } from "./UserCard.lume"

component App {
  view {
    UserCard(user=currentUser)
  }
}
```

---

## 3. 字句仕様

### 3.1 文字コード

Lume ソースは UTF-8 とする。

### 3.2 空白

空白、タブ、改行はトークン区切りとして扱う。

インデントは意味を持たない。

```lume
component A { view { Text("A") } }
```

と

```lume
component A {
  view {
    Text("A")
  }
}
```

は同一である。

### 3.3 コメント

単一行コメント。

```lume
// comment
```

複数行コメント。

```lume
/* comment */
```

ネストされた複数行コメントは v0.1 では禁止する。

### 3.4 識別子

識別子は Unicode 文字を許可する。ただし、生成先の言語で不正な場合、コンパイラは内部的に安全な名前へ変換する。

推奨形式は以下。

```txt
ComponentName
camelCase
snake_case
```

予約語は識別子として使用できない。

---

## 4. キーワード

Lume v0.1 では、キーワードを以下の 3 種類に分ける。

1. 予約語: どの文脈でも識別子に使えない
2. 文脈依存キーワード: 特定構文でのみ特別扱いされる
3. modifier: 宣言や文に付与する修飾子

### 4.1 予約語

```txt
app
module
package
import
export
type
component
page
route
layout
router
server
metadata
search
form
validate
query
mutation
action
effect
state
derived
view
style
theme
slot
if
else
for
in
match
case
default
return
true
false
null
undefined
async
await
unsafe
ffi
struct
enum
trait
opaque
i18n
```

### 4.2 文脈依存キーワード

```txt
on
emit
as
from
auth
csrf
rateLimit
transaction
runtime
invalidates
maxBodySize
language
library
namespace
abi
target
sources
includeDirs
cflags
cxxflags
link
threadSafe
lock
safe
packed
repr
align
throws
errno
worker
header
redirect
notFound
```

### 4.3 modifier

```txt
public
private
readonly
async
server
unsafe
packed
repr
align
throws
```

---

## 5. 基本構造

Lume プログラムは宣言の集合である。

```lume
app ScratchJP {
  theme "./theme.lume"
  routes "./routes.lume"
}
```

主な宣言は以下。

```txt
app
component
page
route
layout
theme
import
export
```

---

## 6. モジュール

Lume のモジュールシステムは、`.lume` ファイル、標準モジュール、FFI モジュール、生成済み runtime module を統一的に扱う。

JavaScript / TypeScript の import 解決には依存しない。Lume コンパイラが Rust 側で独自に解決する。

つまり、`node_modules` を見に行って祈る設計にはしない。そこまで行くと結局いつものフロントエンドである。

---

### 6.1 モジュールの種類

```txt
source module     .lume ソースファイル
std module        Lume 標準モジュール
ffi module        C / C++ FFI 定義
virtual module    コンパイラが生成する内部モジュール
asset module      CSS / image / font などの asset
```

---

### 6.2 module 宣言

ファイル先頭で module 名を宣言できる。

```lume
module app.components.UserCard
```

省略した場合、ファイルパスから module id を推論する。

```txt
src/components/UserCard.lume -> app.components.UserCard
```

---

### 6.3 import

```lume
import { Button, Card } from "lume/std/ui"
import { route, redirect } from "lume/std/router"
import { UserCard } from "./components/UserCard.lume"
import type { User } from "./types.lume"
```

`import type` は型検査専用であり、出力コードには含めない。

JavaScript / TypeScript module の import は禁止する。

```lume
import { something } from "npm:some-package" // error
import { x } from "./x.js"                  // error
```

---

### 6.4 namespace import

```lume
import * as UI from "lume/std/ui"

component App {
  view {
    UI.Button("Save")
  }
}
```

---

### 6.5 export

```lume
export component UserCard(user: User) {
  view {
    Text(user.name)
  }
}
```

型の export。

```lume
export type User = {
  id: String
  name: String
}
```

FFI module の export。

```lume
export ffi module image {
  language "c"
  library "./native/libimage.so"

  fn resize(input: Borrowed<Bytes>, width: i32, height: i32): Owned<Bytes> free=image_free
  fn image_free(ptr: Ptr<u8>): Void
}
```

---

### 6.6 public / private

宣言は既定で module private とする。

外部から参照する宣言には `export` を付ける。

```lume
component InternalButton {
  view { Button("internal") }
}

export component PublicButton {
  view { InternalButton() }
}
```

---

### 6.7 package 宣言

Lume package は `lume.pkg.toml` で定義する。

```toml
[package]
name = "scratchjp-ui"
version = "0.1.0"
edition = "2026"

[exports]
"." = "src/app.lume"
"./ui" = "src/components/mod.lume"
"./theme" = "src/theme.lume"

[dependencies]
# ui-kit = { version = "0.2" }
# local-kit = { path = "../local-kit" }
```

アプリケーション単体では `lume.toml` だけでもよい。

再利用可能なライブラリとして公開する場合は `lume.pkg.toml` を置く。

---

### 6.8 module resolution

import path は以下の順に解決する。

```txt
1. relative path: ./, ../
2. project absolute alias: app/...
3. standard module: lume/std/...
4. package dependency
5. generated virtual module
```

---

### 6.9 標準モジュール一覧

Lume v0.1 は以下の標準モジュールを持つ。

```txt
lume/std/ui
lume/std/layout
lume/std/router
lume/std/form
lume/std/a11y
lume/std/query
lume/std/action
lume/std/asset
lume/std/i18n
lume/std/time
lume/std/result
lume/std/ffi
```

#### 6.9.1 lume/std/ui

```txt
Text
Button
Input
TextArea
Image
Form
Modal
Dialog
Tabs
Table
Anchor
```

#### 6.9.2 lume/std/layout

```txt
Box
Row
Column
Grid
Stack
Container
Spacer
```

#### 6.9.3 lume/std/router

```txt
Router
Route
Link
NavLink
Outlet
navigate
redirect
notFound
useRoute
useParams
useSearchParams
prefetchRoute
```

`useRoute` などはコンパイラ組み込みの router binding へ lowering される。JavaScript 関数として埋め込まれるわけではない。

#### 6.9.4 lume/std/result

```txt
Result<T,E>
Ok<T>
Err<E>
Option<T>
Some<T>
None
```

#### 6.9.5 lume/std/ffi

```txt
Owned<T>
Borrowed<T>
View<T>
Handle<T>
Ptr<T>
ConstPtr<T>
StatusCode
```

---

### 6.10 prelude

以下は既定で import 済みとして扱う。

```txt
Bool
String
Bytes
Array
Option
Result
Text
Button
Input
Image
Box
Row
Column
Grid
Link
Form
```

prelude を無効化できる。

```lume
module app strict
```

`strict` module ではすべての標準名を明示 import する。

---

### 6.11 循環依存

型のみの循環依存は許可する。

値、component、state、action、ffi module の循環依存はエラーとする。

```txt
LUME6001: cyclic module dependency
```

---

### 6.12 Module IR

```rust
pub struct ModuleIr {
    pub id: ModuleId,
    pub source_path: SourcePath,
    pub imports: Vec<ImportIr>,
    pub exports: Vec<ExportIr>,
    pub declarations: Vec<DeclId>,
    pub kind: ModuleKind,
}

pub enum ModuleKind {
    Source,
    Std,
    Ffi,
    Virtual,
    Asset,
}
```

---

### 6.13 モジュール診断

```txt
LUME6001: cyclic module dependency
LUME6002: unresolved import
LUME6003: ambiguous export
LUME6004: private declaration imported from another module
LUME6005: standard module item not available in current target
LUME6006: ffi module imported into client-only component
LUME6007: package export is not declared in lume.pkg.toml
LUME6008: duplicate module declaration
LUME6009: JavaScript module import is not allowed
LUME6010: raw JavaScript embedding is not allowed
```

---

## 7. 型システム

Lume の型システムは、表層言語としての安全性だけでなく、LLVM IR へ安定して lowering できることを目的とする。

そのため、Lume の型は以下の 3 層に分ける。

```txt
Source Type: Lume ソース上で開発者が使う型
Core Type: 型検査後の正規化された内部型
ABI Type: LLVM / WASM / Native / FFI 境界で使う実体表現
```

この分離をしないと、`String` が UI 文字列なのか UTF-8 pointer なのか GC 管理値なのか C string なのか分からなくなる。つまり、いつもの人類製型システム災害である。

---

### 7.1 型設計の原則

Lume の型システムは以下の原則に従う。

1. Source Type は書きやすさを優先する
2. Core Type は意味の一意性を優先する
3. ABI Type は LLVM lowering の安定性を優先する
4. FFI 境界では曖昧な型を禁止する
5. Server Action 境界では直列化可能性を型で検査する
6. UI state は reactive storage へ lowering 可能でなければならない
7. Native / WASM 生成では layout が決まらない型を残さない

---

### 7.2 型の分類

Lume の型は大きく以下に分類される。

```txt
Primitive Type
Numeric Type
Text Type
Collection Type
Record Type
Union Type
Option Type
Result Type
Function Type
UI Type
Resource Type
Pointer / FFI Type
Opaque Type
Never / Unknown / Any
```

### 7.2.1 v0.1 の built-in 型

v0.1 で言語組み込みとして保証する型は以下に限定する。

```txt
Bool
Void
i8 i16 i32 i64
u8 u16 u32 u64
isize usize
f32 f64
String
Bytes
Array<T>
Option<T>
Result<T, E>
```

`Map`、`Unknown`、`Any` などの拡張型は既定では禁止し、`experimental` フラグでのみ許可する。

```txt
LUME5016: experimental type requires feature flag
```

---

### 7.3 Primitive Type

```txt
Bool
Void
Null
Undefined
Never
Unknown
Any
```

#### 7.3.1 Bool

`Bool` は真偽値である。

Core Type。

```txt
bool
```

LLVM 内部表現。

```txt
i1
```

ABI 境界では `i8` に拡張できる。

```txt
Bool internal -> i1
Bool ABI      -> i8, 0 = false, 1 = true
```

#### 7.3.2 Void

`Void` は値を返さないことを表す。

LLVM 表現。

```txt
void
```

#### 7.3.3 Never

`Never` は戻らない制御フローを表す。

例。

```lume
redirect("/login")
throw error
abort()
```

LLVM lowering では `unreachable` を使える。

```llvm
unreachable
```

#### 7.3.4 Unknown

`Unknown` は型が不明な値である。

LLVM lowering 前に必ず具体型へ narrow されなければならない。

```lume
const value: Unknown = input
```

`Unknown` が Backend IR に残る場合はエラー。

```txt
LUME5001: Unknown type must be narrowed before LLVM lowering
```

#### 7.3.5 Any

`Any` は型検査を弱める脱出口である。

`Any` は JS glue 側では許可されるが、WASM / Native / JIT backend へ lowering する場合は runtime tagged value へ変換するか、明示 cast が必要である。

```txt
Any -> LumeValue tagged union
```

`max-runtime` では `Any` の使用を警告する。

---

### 7.4 数値型

Lume は LLVM lowering を考慮し、明示的な固定幅数値型を持つ。

```txt
i8
i16
i32
i64
u8
u16
u32
u64
isize
usize
f32
f64
```

高級別名。

```txt
Int    = i64 source default
Float  = f64
Number = f64
```

`Int` は Source Type としては許可されるが、FFI / ABI / packed struct では禁止または警告する。

```lume
fn add(a: Int, b: Int): Int // warning at ABI boundary
```

FFI では次を推奨する。

```lume
fn add(a: i32, b: i32): i32
```

#### 7.4.1 LLVM 対応

```txt
i8    -> i8
i16   -> i16
i32   -> i32
i64   -> i64
u8    -> i8
u16   -> i16
u32   -> i32
u64   -> i64
isize -> target pointer width integer
usize -> target pointer width integer
f32   -> float
f64   -> double
```

符号付き / 符号なしの違いは LLVM integer type 自体には存在しない。

演算命令、比較命令、拡張命令で区別する。

```txt
signed compare   -> icmp slt / sgt / sle / sge
unsigned compare -> icmp ult / ugt / ule / uge
signed extend    -> sext
unsigned extend  -> zext
```

#### 7.4.2 オーバーフロー

標準では、Source Type の整数演算は debug / fast-build で overflow check を入れる。

```txt
fast-build: checked arithmetic
max-runtime: profile setting に従う
```

設定。

```toml
[numeric]
overflow = "checked" # checked | wrapping | trap | unchecked
```

LLVM lowering。

```txt
checked   -> llvm.sadd.with.overflow.* intrinsic
wrapping  -> add / sub / mul
trap      -> overflow branch + trap
unchecked -> add nsw / nuw where proven safe
```

---

### 7.5 Text Type

文字列型は複数の表現を持つ。

```txt
String
Utf8String
Utf16String
cstring
```

#### 7.5.1 String

`String` は Lume の標準文字列型である。

Source Type では抽象型だが、Core Type では以下へ正規化される。

```txt
String -> LumeString
```

標準 ABI 表現。

```txt
LumeString = {
  ptr: *u8,
  len: usize,
  encoding: StringEncoding,
  flags: u32
}
```

既定 encoding は UTF-8 とする。

```txt
StringEncoding.utf8 = 1
StringEncoding.utf16 = 2
```

LLVM 構造体。

```llvm
%LumeString = type { ptr, i64, i32, i32 }
```

32-bit target では `usize` は `i32` になる。

#### 7.5.2 Utf8String

`Utf8String` は長さ付き UTF-8 文字列である。

ABI 表現。

```txt
{ ptr: *u8, len: usize }
```

LLVM。

```llvm
%LumeUtf8String = type { ptr, i64 }
```

#### 7.5.3 Utf16String

`Utf16String` は長さ付き UTF-16 code unit 列である。

ABI 表現。

```txt
{ ptr: *u16, len: usize }
```

LLVM。

```llvm
%LumeUtf16String = type { ptr, i64 }
```

#### 7.5.4 cstring

`cstring` は NUL 終端 UTF-8 とする。

ABI 表現。

```txt
*const i8
```

Lume `String` から `cstring` へ変換する場合、NUL byte を含む文字列はエラー。

---

### 7.6 Bytes

`Bytes` はバイト列である。

Source Type。

```txt
Bytes
```

ABI 表現。

```txt
{ ptr: *u8, len: usize }
```

所有権を持つ場合。

```txt
OwnedBytes = { ptr: *u8, len: usize, cap: usize }
```

LLVM。

```llvm
%LumeBytes = type { ptr, i64 }
%LumeOwnedBytes = type { ptr, i64, i64 }
```

---

### 7.7 Collection Type

#### 7.7.1 Array

```lume
state users: User[] = []
```

Core Type。

```txt
Array<User>
```

ABI 表現。

```txt
LumeArray<T> = {
  ptr: *T,
  len: usize,
  cap: usize
}
```

LLVM。

```llvm
%LumeArray_User = type { ptr, i64, i64 }
```

WASM では pointer は linear memory offset `i32` として表される。

#### 7.7.2 FixedArray<T, N>

固定長配列。

```lume
type Vec3 = FixedArray<f32, 3>
```

LLVM。

```llvm
[3 x float]
```

FFI struct 内で使用できる。

#### 7.7.3 Map<K, V>

`Map` は高級型であり、Native / WASM へ lowering するには runtime 実装が必要である。

ABI 境界では禁止する。

```txt
LUME5002: Map cannot cross FFI boundary; convert to Array<Record> or Bytes
```

---

### 7.8 Record Type

Record は名前付き field を持つ構造体である。

```lume
type User = {
  id: String
  name: String
  xp: i64
}
```

Core Type。

```txt
Record User {
  id: LumeString
  name: LumeString
  xp: i64
}
```

LLVM。

```llvm
%User = type { %LumeString, %LumeString, i64 }
```

#### 7.8.1 Layout

通常の Record は Lume layout を使う。

FFI に渡す場合は `repr="C"` を要求する。

```lume
type UserC repr="C" = {
  id: u64
  xp: i32
}
```

`repr="C"` の場合、LLVM DataLayout に従って padding / alignment を計算する。

#### 7.8.2 Packed Record

```lume
type Header repr="C" packed align=1 = {
  magic: u32
  version: u16
  flags: u16
}
```

LLVM。

```llvm
%Header = type <{ i32, i16, i16 }>
```

---

### 7.9 Optional / Nullable

Lume の optional は `T?` と書く。

```lume
image?: URL
```

Core Type。

```txt
Option<T>
```

#### 7.9.1 Nullable pointer optimization

`T` が pointer-like な場合、`Option<T>` は null pointer で表せる。

```txt
Option<Handle<T>> -> ptr, null = none
Option<cstring>   -> ptr, null = none
```

#### 7.9.2 Tagged representation

値型の場合は tagged union にする。

```txt
Option<i32> = { tag: i1, value: i32 }
```

LLVM。

```llvm
%Option_i32 = type { i1, i32 }
```

---

### 7.10 Union Type

Union は複数候補のいずれかを表す。

```lume
type Status = "idle" | "loading" | "success" | "error"
```

文字列 literal union は enum に lowering できる。

```txt
idle    -> 0
loading -> 1
success -> 2
error   -> 3
```

LLVM。

```txt
i8 / i16 / i32 depending on variant count
```

値を持つ union。

```lume
type LoadState =
  | { kind: "loading" }
  | { kind: "error", message: String }
  | { kind: "success", data: User }
```

Core Type。

```txt
TaggedUnion LoadState
```

ABI 表現。

```txt
{ tag: u32, payload: union_payload }
```

LLVM では最大 payload size に合わせた byte array を使う。

```llvm
%LoadState = type { i32, [N x i8] }
```

必要に応じて payload pointer 表現にする。

---

### 7.11 Result<T, E>

`Result<T, E>` は成功 / 失敗を表す標準型である。

```lume
type Result<T, E> = {
  ok: Bool
  value?: T
  error?: E
}
```

Core Type では専用型に正規化される。

```txt
Result<T,E> = tagged union {
  Ok(T)
  Err(E)
}
```

LLVM 表現。

```txt
{ tag: i1, payload: max(sizeof(T), sizeof(E)) }
```

Server Action の戻り値では、wire format へ encode される。

---

### 7.12 Function Type

Lume の通常関数型。

```lume
(value: String) => Void
```

Core Type。

```txt
Function(params, return)
```

LLVM lowering では以下のどちらかになる。

```txt
direct function pointer
closure object { fn_ptr, env_ptr }
```

capture がない場合。

```txt
fn pointer
```

capture がある場合。

```txt
Closure<T> = { fn_ptr: ptr, env_ptr: ptr }
```

UI event handler は直接 LLVM function pointer として扱わず、dispatch table の action id へ lowering される。

---

### 7.13 UI Type

UI ノードは通常の実行時値ではなく、コンパイラ管理の型である。

```txt
Node
Element
ComponentInstance
ViewBlock
Slot
```

これらは LLVM の値として直接扱わない。

`view` はコンパイル時に HTML template、binding table、patch function へ lowering される。

```txt
ViewBlock -> Template IR + Binding IR + Patch IR
```

したがって、以下は原則禁止する。

```lume
const node: Node = Text("hello") // restricted
```

UI Type を通常の変数として保持できるのは compile-time macro / template function 内に限る。

---

### 7.14 Resource Type

Resource Type は明示的な解放や runtime 管理が必要な値である。

```txt
File
Stream<T>
Handle<T>
Owned<T>
Borrowed<T>
View<T>
```

これらは Server Action / FFI / Native backend で重要になる。

#### 7.14.1 Owned

所有権を持つ値。

```lume
Owned<Bytes>
```

LLVM lowering では cleanup block を生成できる。

```txt
normal exit -> free if not moved
error exit  -> free if initialized
```

#### 7.14.2 Borrowed

借用値。

呼び出し中のみ有効。

LLVM では pointer / length などとして渡すが、escape analysis でスコープ外流出を禁止する。

#### 7.14.3 View

copy なし参照。

非同期境界を越えてはならない。

```txt
LUME5003: View<T> cannot cross async boundary
```

---

### 7.15 Pointer / FFI Type

FFI 用低レベル型。

```txt
Ptr<T>
ConstPtr<T>
Opaque<T>
Handle<T>
```

LLVM。

```txt
Ptr<T>      -> ptr
ConstPtr<T> -> ptr
Opaque<T>   -> ptr to opaque type
Handle<T>   -> ptr or integer handle depending on backend
```

Pointer は Source Type として一般 UI コードに出してはならない。

```txt
LUME5004: pointer type is only allowed in server-only or ffi context
```

---

### 7.16 型正規化

型検査後、Source Type は Core Type へ正規化される。

```txt
Int       -> i64
Float     -> f64
Number    -> f64
String    -> LumeString
T?        -> Option<T>
A | B     -> Union<A,B>
{...}     -> Record
T[]       -> Array<T>
```

例。

```lume
type User = {
  id: String
  xp: Int
  image?: URL
}
```

Core Type。

```txt
Record User {
  id: LumeString
  xp: i64
  image: Option<LumeUrl>
}
```

---

### 7.17 ABI lowering

Core Type は backend ごとに ABI Type へ lowering される。

#### 7.17.1 JS glue ABI

JS glue では通常の JavaScript value として扱う。

```txt
Bool   -> boolean
String -> string
Array  -> Array
Record -> object
Bytes  -> Uint8Array
```

#### 7.17.2 WASM ABI

WASM ABI では pointer / length / numeric に変換する。

```txt
Bool       -> i32
Integer    -> i32 / i64
Float      -> f32 / f64
String     -> ptr,len
Bytes      -> ptr,len
Array<T>   -> ptr,len
Record     -> pointer to memory layout
Result     -> pointer to tagged result
```

WASM MVP では複合値を複数戻り値で返すより、out pointer か allocated result pointer を使う。

#### 7.17.3 Native ABI

Native ABI では LLVM DataLayout と target ABI に従う。

小さな primitive は値渡し、大きな record / union / array は pointer 渡しを基本とする。

```txt
small primitive -> by value
large record    -> by pointer
String          -> by pointer or pair depending on calling convention
Owned<T>        -> by pointer with ownership metadata
```

#### 7.17.4 FFI ABI

FFI ABI では `repr="C"` または明示 ABI 型を要求する。

```txt
String is forbidden unless Utf8String / cstring specified
Int is discouraged; use i32/i64
Record requires repr="C"
Union requires repr="C" enum or manual tagged struct
```

---

### 7.18 LLVM lowering table

| Lume Core Type | LLVM Type                      | 備考                     |
| -------------- | ------------------------------ | ---------------------- |
| bool           | i1                             | ABI では i8/i32 に拡張可     |
| i8/u8          | i8                             | 符号は命令で区別               |
| i16/u16        | i16                            | 同上                     |
| i32/u32        | i32                            | 同上                     |
| i64/u64        | i64                            | 同上                     |
| isize/usize    | pointer width int              | target dependent       |
| f32            | float                          | IEEE 754               |
| f64            | double                         | IEEE 754               |
| Void           | void                           | 戻り値なし                  |
| Never          | unreachable                    | 制御フローなし                |
| LumeString     | struct                         | ptr,len,encoding,flags |
| Utf8String     | struct                         | ptr,len                |
| Utf16String    | struct                         | ptr,len                |
| Bytes          | struct                         | ptr,len                |
| Array          | struct                         | ptr,len,cap            |
| Record         | struct                         | layout に依存             |
| Packed Record  | packed struct                  | `<{ ... }>`            |
| Option         | nullable ptr or tagged struct  | T に依存                  |
| Union          | tagged payload                 | tag + payload          |
| Result<T,E>    | tagged payload                 | Ok/Err                 |
| Function       | function ptr or closure struct | capture に依存            |
| Ptr            | ptr                            | opaque pointer         |
| Handle         | ptr / integer                  | backend dependent      |

---

### 7.19 型レイアウト計算

Native / WASM / FFI では型サイズと alignment を計算する。

```ts
interface TypeLayout {
  size: number
  align: number
  fields?: FieldLayout[]
}
```

LLVM backend では target DataLayout を使う。

```txt
x86_64-unknown-linux-gnu
wasm32-unknown-unknown
wasm32-wasi
```

target が異なると layout が変わる可能性があるため、layout cache key には target triple を含める。

---

### 7.20 Generics

Generic 型は LLVM lowering 前に単相化する。

```lume
type Box<T> = {
  value: T
}
```

使用。

```lume
Box<i32>
Box<String>
```

生成される Core Type。

```txt
Box_i32
Box_LumeString
```

これを monomorphization と呼ぶ。

`Any` ベースの dynamic generic は Native / WASM では tagged value runtime を必要とする。

`max-runtime` では monomorphization を優先する。

---

### 7.21 Trait / Interface

v0.1 では完全な trait system は必須ではない。

ただし、最適化と LLVM lowering のため、制約付き generic を将来拡張として予約する。

```lume
trait Serializable<T> {
  fn encode(value: T): Bytes
}
```

Backend IR では dictionary passing または monomorphization へ lowering する。

```txt
fast-build: dictionary passing allowed
max-runtime: monomorphization preferred
```

---

### 7.22 型と直列化

Server Action 境界を越える型は `Serializable` でなければならない。

許可。

```txt
Bool
number types
String
Bytes
Array<T: Serializable>
Record with Serializable fields
Option<T>
Result<T,E>
literal union
tagged union
```

禁止。

```txt
Ptr<T>
Handle<T>
Function
Closure
View<T>
Borrowed<T>
Opaque<T>
```

ただし `Handle<T>` は明示的な external id へ変換する adapter がある場合のみ許可する。

---

### 7.23 型と GC / 所有権

Lume Native backend は GC を必須としない。

標準戦略は所有権 + 参照カウント + arena の組み合わせである。

```txt
短命 UI 計算 -> arena
String / Bytes -> ref counted or owned buffer
Server Action local -> stack / arena
FFI Owned<T> -> explicit free
```

LLVM lowering では必要に応じて retain / release を挿入する。

```txt
LumeString retain on capture
LumeString release on scope exit
Owned<T> free on cleanup unless moved
```

`fast-build` では安全性のため retain/release を多めに挿入してよい。

`max-runtime` では escape analysis で削減する。

---

### 7.24 型診断

```txt
LUME5001: Unknown type must be narrowed before LLVM lowering
LUME5002: Map cannot cross FFI boundary
LUME5003: View<T> cannot cross async boundary
LUME5004: pointer type is only allowed in server-only or ffi context
LUME5005: Int is ambiguous at ABI boundary; use i32 or i64
LUME5006: Record crossing FFI boundary requires repr="C"
LUME5007: Union layout is not ABI-stable
LUME5008: generic type was not monomorphized before backend lowering
LUME5009: non-serializable type used in Server Action boundary
LUME5010: closure cannot cross WASM ABI boundary
LUME5011: owned resource may leak on error path
LUME5012: borrowed value escapes its lifetime
LUME5013: packed field may be unaligned on target backend
LUME5014: numeric overflow behavior is unspecified
LUME5015: Any requires tagged runtime representation in native backend
```

---

