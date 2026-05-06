## 25. データ取得

この章は、UI から外部データやネイティブ実装へつながる経路をまとめる。`query`、Server Actions、フォーム、C / C++ FFI をここで整理する。

```txt
view
  -> query / action
  -> server / native bridge
  -> cached result / re-render
```

フロントエンドの状態更新だけでなく、サーバー側の処理や既存資産の呼び出しをどう一つのモデルで扱うかに注目すると読みやすい。

現行実装では `query` の client cache と `server action` からの簡易 invalidation は実装済みで、`mutation`、`stream`、`validation DSL` は [未実装] とみなす。

### 25.1 query

```lume
query ranking = api.get("/ranking")
```

query は以下の状態を持つ。

```txt
loading: Bool
error: Error?
data: T?
refetch(): Void
```

### 25.2 mutation [未実装]

```lume
mutation saveUser = api.post("/users")
```

使用例。

```lume
Button("Save") {
  on click {
    await saveUser.mutate(form)
  }
}
```

### 25.3 cache key

```lume
query user key=["user", id] = api.get("/users/{id}")
```

HTML + JS 出力では、`query` は Lume client runtime の fetch/cache 層へ変換される。HTML + JS + WASM 出力では、cache key 計算や response validation を WASM 側に置ける。

実装では `query key` は `JSON.stringify` ベースの簡易 cache key として扱う。`server query` は AST / manifest 上に残るが、専用の実行路はまだない。

---

## 26. Server Actions

Server Actions は、クライアント UI から呼び出せるサーバー側関数である。

Lume における Server Actions は、以下を目的とする。

1. フォーム送信や状態変更をサーバー側で安全に実行する
2. クライアントバンドルに秘密情報や DB 接続コードを含めない
3. 入力と戻り値を型検査する
4. 権限、CSRF、レート制限、トランザクションを宣言的に扱う
5. Lume Native / JIT backend 上の RPC、form action、streaming action へ変換可能にする

人類はなぜ毎回 API route と client fetch と validation を別々に書くのか。Lume では少なくとも同じ地獄を 3 回書かない設計にする。

---

### 26.1 server action 宣言

```lume
server action createPost(input: CreatePostInput): Post {
  const user = await auth.requireUser()

  return await db.post.create({
    title: input.title,
    body: input.body,
    authorId: user.id
  })
}
```

`server action` は必ずサーバー環境でのみ実行される。

クライアント出力では、直接関数本体を含めず、呼び出し用のスタブに変換する。

---

### 26.2 入力型

Server Action の引数は直列化可能でなければならない。

許可される型。

```txt
String
Int
Float
Number
Bool
Null
DateTime
URL
Array<T>
Object
Union
Optional
File
FormData
```

禁止される型。

```txt
Function
Class instance
Symbol
DOM Node
Stream, unless explicitly marked
Pointer, unless FFI boundary type
```

例。

```lume
type CreatePostInput = {
  title: String
  body: String
  tags?: String[]
}
```

---

### 26.3 戻り値型

Server Action の戻り値も直列化可能でなければならない。

```lume
server action getUser(id: String): User {
  return await db.user.find(id)
}
```

戻り値なしの場合。

```lume
server action deletePost(id: String): Void {
  await db.post.delete(id)
}
```

---

### 26.4 クライアントからの呼び出し

```lume
component CreatePostForm {
  state title: String = ""
  state body: String = ""
  state pending: Bool = false

  view {
    Form {
      Input(label="Title", value=title) {
        on input(value) {
          title = value
        }
      }

      TextArea(label="Body", value=body) {
        on input(value) {
          body = value
        }
      }

      Button("Create", disabled=pending) {
        on click async {
          pending = true
          await createPost({ title, body })
          pending = false
        }
      }
    }
  }
}
```

コンパイラは `createPost` の呼び出しを、Lume client runtime の RPC stub または form action へ変換する。framework-native server action へは変換しない。

---

### 26.5 form action

`Form` に Server Action を直接 bind できる。

```lume
server action login(form: FormData): LoginResult {
  const email = form.getString("email")
  const password = form.getString("password")

  return await auth.login(email, password)
}

component LoginPage {
  view {
    Form action=login method="post" {
      Input(name="email", label="Email")
      Input(name="password", label="Password", type="password")
      Button("Login", type="submit")
    }
  }
}
```

この形式では progressive enhancement をサポートする。

JavaScript が無効な環境でも、通常の POST として動作できるターゲットを許可する。

---

### 26.6 validation [未実装]

Server Action には入力検証を指定できる。

```lume
server action createUser(input: CreateUserInput): User
  validate {
    input.email: required email
    input.password: required minLength(8)
    input.name: required maxLength(64)
  }
{
  return await db.user.create(input)
}
```

validation に失敗した場合、Action は実行されない。

エラーは標準形式で返る。

```lume
type ActionError = {
  code: String
  message: String
  field?: String
}
```

---

### 26.7 result 型 [未実装]

Server Action は例外を投げる形式と `Result<T, E>` 形式の両方をサポートする。

推奨は `Result` である。UI 側の分岐が明確になるためである。人間には例外が見えないとすぐ握りつぶす習性がある。

```lume
type Result<T, E> = {
  ok: Bool
  value?: T
  error?: E
}
```

```lume
server action updateProfile(input: ProfileInput): Result<User, ActionError> {
  if input.name.trim() == "" {
    return {
      ok: false,
      error: {
        code: "EMPTY_NAME",
        message: "Name is required",
        field: "name"
      }
    }
  }

  return {
    ok: true,
    value: await db.user.update(input)
  }
}
```

---

### 26.8 auth

Server Action は認証要件を宣言できる。

```lume
server action createPost(input: CreatePostInput): Post
  auth required
{
  const user = auth.user
  return await db.post.create({ ...input, authorId: user.id })
}
```

ロール指定。

```lume
server action deleteUser(id: String): Void
  auth role="admin"
{
  await db.user.delete(id)
}
```

権限関数。

```lume
server action updatePost(id: String, input: UpdatePostInput): Post
  auth can="post:update"
{
  return await db.post.update(id, input)
}
```

実装では `auth required` 相当の強制認証と、`auth optional` の緩和までは扱う。`role=`、`can=` の細かな権限分岐は [未実装]。

---

### 26.9 CSRF

ブラウザから呼び出される mutation 系 Server Action は、既定で CSRF 保護を有効にする。

```lume
server action updateSettings(input: SettingsInput): Settings
  csrf true
{
  return await db.settings.update(input)
}
```

明示的に無効化する場合。

```lume
server action webhook(input: WebhookPayload): Void
  csrf false
{
  await handleWebhook(input)
}
```

`csrf false` は warning を出す。

---

### 26.10 rate limit [一部実装]

```lume
server action sendMessage(input: MessageInput): Message
  rateLimit {
    key: auth.user.id
    limit: 10
    window: 1m
  }
{
  return await db.message.create(input)
}
```

単位。

```txt
ms
s
m
h
d
```

実装では `rateLimit` の存在は判定するが、`key` / `limit` / `window` の細かな指定はまだ runtime に降りていない。

---

### 26.11 transaction [未実装]

```lume
server action transfer(input: TransferInput): Void
  transaction
{
  await db.account.debit(input.from, input.amount)
  await db.account.credit(input.to, input.amount)
}
```

名前付きトランザクション。

```lume
server action createOrder(input: OrderInput): Order
  transaction isolation="serializable"
{
  const order = await db.order.create(input)
  await db.inventory.reserve(input.items)
  return order
}
```

---

### 26.12 runtime [一部実装]

Server Action の実行環境を指定できる。

```lume
server action resizeImage(file: File): ImageResult
  runtime "native"
{
  return image.resize(file)
}
```

```lume
server action computeScore(input: ScoreInput): i64
  runtime "jit"
{
  return score.compute(input)
}
```

標準 runtime。

```txt
native
jit
```

`native` は AOT コンパイルされた Lume server binary 上で実行される。

`jit` は Lume IR / Backend IR を実行時にコンパイルして実行する。

FFI を使用する Server Action は原則 `native` を要求する。JIT から FFI を使う場合は、FFI symbol table の lazy resolution と安全性診断を必須にする。

実装では `native` と `jit` の宣言は manifest に反映されるが、`jit` の実行は現状 i64 系の簡易 action に限定される。

---

### 26.13 streaming [未実装]

Server Action は stream を返せる。

```lume
server action generateText(prompt: String): Stream<String> {
  return await ai.stream(prompt)
}
```

クライアント側。

```lume
on click async {
  for await chunk in generateText(prompt) {
    output += chunk
  }
}
```

HTTP 出力では SSE / fetch streaming / WebSocket のいずれかへ変換できる。

---

### 26.14 redirect / revalidate [一部実装]

```lume
server action createPost(input: CreatePostInput): Void {
  const post = await db.post.create(input)
  revalidate("/posts")
  redirect("/posts/{post.id}")
}
```

`redirect` は action の実行を終了する制御フローとして扱う。

実装では `revalidate(...)` / `invalidates` は `revalidate` 配列として返却できるが、`redirect` の制御フローはまだない。

---

### 26.15 キャッシュ無効化

```lume
server action updateUser(input: UserInput): User
  invalidates ["user", input.id]
{
  return await db.user.update(input)
}
```

複数指定。

```lume
invalidates [
  ["user", input.id],
  ["ranking"],
  "/users/{input.id}"
]
```

この項目は manifest には出るが、クライアント runtime 側の一般化された cache invalidation は [未実装] で、現状は server action 呼び出し時の簡易 invalidation に留まる。

---

### 26.16 ファイルアップロード [未実装]

```lume
server action uploadAvatar(file: File): URL
  auth required
  maxBodySize 5MB
{
  return await storage.upload(file)
}
```

複数ファイル。

```lume
server action uploadImages(files: File[]): URL[]
  maxBodySize 50MB
{
  return await storage.uploadMany(files)
}
```

実装では `maxBodySize` のサイズ制限はあるが、multipart / File binding の本格的なアップロード処理は [未実装]。

---

### 26.17 Server Action のコンパイルモデル

Lume は Server Action を以下に分離する。

```txt
server implementation
client stub
type schema
transport binding
runtime manifest
```

実装では `lume.manifest.json` に加えて `lume.backend.json` を生成し、`actions`、`runtime`、`csrf`、`invalidates`、`maxBodySize`、`rateLimit`、`transaction` を運搬する。

例。

```lume
server action add(a: Int, b: Int): Int {
  return a + b
}
```

クライアントスタブ概念。

```ts
export async function add(a: number, b: number): Promise<number> {
  return callServerAction("add", [a, b])
}
```

サーバー登録概念。

```ts
registerServerAction("add", async (a, b) => a + b)
```

---

### 26.18 Server Action Manifest

コンパイラは Server Action の manifest を生成する。

```json
{
  "actions": [
    {
      "id": "createPost",
      "module": "src/actions/post.lume",
      "runtime": "native",
      "auth": "required",
      "csrf": true,
      "input": "CreatePostInput",
      "output": "Post"
    }
  ]
}
```

現行の manifest では、各 action に `id`、`runtime`、`auth`、`csrf`、`input`、`output`、`invalidates`、`maxBodySize`、`rateLimit`、`transaction` を含める。

---

### 26.19 Server Action 診断

```txt
LUME2001: server action uses non-serializable input
LUME2002: server action return type is not serializable
LUME2003: secret value leaked into client bundle
LUME2004: mutation action missing csrf protection
LUME2005: jit runtime cannot use this native FFI without an enabled FFI bridge
LUME2006: action requires auth but no auth provider configured
LUME2007: transaction requested but target runtime has no transaction adapter
LUME2008: file upload exceeds configured body limit
```

---

## 27. C / C++ FFI

Lume は C / C++ 関数を Server Action や server-only module から呼び出すための FFI を定義する。

FFI は UI 言語に入れるにはやや物騒だが、画像処理、音声処理、数値計算、既存 C ライブラリ連携では必要になる。JavaScript だけで全部やると、最後は npm install の祈祷になる。

---

### 27.1 基本方針

C / C++ FFI は以下の制約を持つ。

1. サーバー専用である
2. クライアントバンドルに含めてはならない
3. 型境界を明示する
4. メモリ所有権を宣言する
5. 例外、errno、戻り値エラーを扱える
6. runtime ごとにバックエンドを切り替えられる
7. Web / edge runtime では原則使用不可

---

### 27.2 ffi module 宣言

```lume
ffi module mathlib {
  language "c"
  library "./native/libmathlib.so"
  header "./native/mathlib.h"

  fn add(a: i32, b: i32): i32
  fn sin_fast(x: f64): f64
}
```

呼び出し。

```lume
server action compute(x: Float): Float
  runtime "native"
{
  return mathlib.sin_fast(x)
}
```

実装では `language`、`library`、`header`、`sources`、`runtime`、`safe` / `unsafe`、`threadSafe`、`lock`、`fn`、`free`、`throws`、`callback` を受ける。`namespace`、`abi`、`out` パラメータ構文は [未実装]。

---

### 27.3 C++ module [未実装]

```lume
ffi module imagecodec {
  language "cpp"
  library "./native/libimagecodec.so"
  header "./native/imagecodec.hpp"
  namespace "imagecodec"
  abi "cxx"

  fn resize(input: Bytes, width: i32, height: i32): Bytes
}
```

C++ ABI はコンパイラや標準ライブラリ差異の影響を受けやすい。

安定性を重視する場合、C ABI ラッパーを推奨する。

---

### 27.4 C ABI 推奨形式 [未実装]

```cpp
extern "C" {
  int image_resize(
    const unsigned char* input_ptr,
    size_t input_len,
    int width,
    int height,
    unsigned char** output_ptr,
    size_t* output_len
  );

  void image_free(unsigned char* ptr);
}
```

Lume 側。

```lume
ffi module image {
  language "c"
  library "./native/libimage.so"

  fn image_resize(
    input: Borrowed<Bytes>,
    width: i32,
    height: i32,
    out output: Owned<Bytes> free=image_free
  ): StatusCode

  fn image_free(ptr: Ptr<u8>): Void
}
```

---

### 27.5 FFI 型

Lume FFI は通常の UI 型とは別に低レベル型を持つ。

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
bool
char
cstring
void
Ptr<T>
ConstPtr<T>
Bytes
Struct<T>
Opaque<T>
Handle<T>
```

Lume 型との対応。

```txt
Int      -> i32 または i64。明示推奨
Float    -> f64
Number   -> f64
Bool     -> bool
String   -> cstring / Utf8String
Bytes    -> pointer + length
File     -> Bytes / Stream<Bytes>
```

`Int` を FFI 境界で使う場合は警告する。

実装では `Ptr<T>`、`ConstPtr<T>`、`Borrowed<T>`、`Owned<T>`、`View<T>`、`Handle<T>`、`Struct<T>`、`Opaque<T>`、`cstring`、`Utf8String`、`Utf16String`、`Bytes` を扱う。`namespace` 付き C++ ABI やプラットフォーム別条件分岐は [未実装]。

```lume
fn add(a: Int, b: Int): Int // warning: use i32 or i64 at FFI boundary
```

---

### 27.6 文字列

### 27.6.1 C string

```lume
fn strlen(value: cstring): usize
```

`cstring` は NUL 終端 UTF-8 とする。

NUL を含む Lume `String` を `cstring` に渡す場合はエラー。

### 27.6.2 UTF-8 string

長さ付き UTF-8 文字列。

```lume
fn normalize(input: Utf8String): Utf8String
```

C 側の推奨 ABI。

```c
typedef struct {
  const char* ptr;
  size_t len;
} lume_utf8_string;
```

### 27.6.3 UTF-16 string

```lume
fn measure_text(input: Utf16String): i32
```

Scratch 系や JS 互換の内部表現では UTF-16 が便利な場合がある。便利というより、過去の互換性という名の化石燃料で動いている。

---

### 27.7 Bytes

```lume
fn hash(input: Bytes): u64
```

C ABI。

```c
typedef struct {
  const unsigned char* ptr;
  size_t len;
} lume_bytes;
```

戻り値としての Bytes は所有権を明示する。

```lume
fn compress(input: Borrowed<Bytes>): Owned<Bytes> free=free_bytes
```

---

### 27.8 ポインタ

```lume
fn process(ptr: Ptr<u8>, len: usize): i32
```

ポインタ型は Server Action の入出力には直接使えない。

```lume
server action bad(): Ptr<u8> {
  return native.alloc(10) // error
}
```

ポインタは FFI module 内、または server-only code 内でのみ有効である。

---

### 27.9 所有権

Lume FFI は所有権を型で表現する。

```txt
Borrowed<T>  呼び出し中だけ有効。解放してはいけない
Owned<T>     呼び出し側が所有。指定された free で解放する
View<T>      コピーなし参照。非同期保持禁止
Handle<T>    opaque resource。close/free が必要
```

例。

```lume
fn parse(data: Borrowed<Bytes>): Owned<Handle<Document>> free=doc_free
fn doc_free(doc: Handle<Document>): Void
```

---

### 27.10 lifetime

FFI から返された借用参照は action の同期実行範囲外へ逃がしてはならない。

```lume
fn get_buffer_view(handle: Handle<Buffer>): View<Bytes>
```

禁止例。

```lume
server action bad(): View<Bytes> {
  return native.get_buffer_view(handle) // error
}
```

コピーして返す必要がある。

```lume
server action good(): Bytes {
  const view = native.get_buffer_view(handle)
  return copy(view)
}
```

---

### 27.11 struct

C struct を定義できる。

```lume
ffi struct Vec2 {
  x: f32
  y: f32
}

ffi module geom {
  language "c"
  library "./native/libgeom.so"

  fn length(v: Vec2): f32
}
```

C 側。

```c
typedef struct {
  float x;
  float y;
} Vec2;
```

---

### 27.12 packed / align

```lume
ffi struct Header packed align=1 {
  magic: u32
  version: u16
  flags: u16
}
```

明示的な layout 指定がない struct は C layout とする。

```lume
ffi struct User repr="C" {
  id: u64
  score: i32
}
```

---

### 27.13 enum

```lume
ffi enum ImageFormat repr="i32" {
  png = 1
  jpeg = 2
  webp = 3
}
```

---

### 27.14 opaque type

```lume
ffi opaque Decoder

ffi module codec {
  language "c"
  library "./native/libcodec.so"

  fn decoder_new(): Owned<Handle<Decoder>> free=decoder_free
  fn decoder_free(decoder: Handle<Decoder>): Void
  fn decoder_decode(decoder: Handle<Decoder>, input: Borrowed<Bytes>): Owned<Bytes> free=bytes_free
}
```

---

### 27.15 error handling

### 27.15.1 status code

```lume
ffi module codec {
  fn decode(input: Borrowed<Bytes>, out output: Owned<Bytes> free=bytes_free): StatusCode
}
```

`StatusCode` が 0 以外の場合、Lume は FFIError に変換できる。

```lume
fn decode(...): StatusCode throws
```

### 27.15.2 errno

```lume
ffi module libc {
  errno true

  fn open(path: cstring, flags: i32): i32 throws errno
}
```

### 27.15.3 custom error function

```lume
ffi module image {
  fn last_error(): cstring

  fn decode(input: Borrowed<Bytes>): Owned<Bytes> free=image_free
    throws last_error
}
```

---

### 27.16 C++ 例外

C++ 例外を FFI 境界から直接越えてはならない。

```lume
ffi module badcpp {
  language "cpp"
  fn risky(): i32 throws cpp_exception // error by default
}
```

C++ 側で例外を捕捉し、C ABI のエラーコードへ変換することを推奨する。

---

### 27.17 callback

C から Lume / JS 側へ callback を呼ぶ場合。

```lume
ffi callback ProgressCallback(current: u64, total: u64): Void

ffi module encoder {
  fn encode(input: Borrowed<Bytes>, progress: ProgressCallback): Owned<Bytes> free=bytes_free
}
```

callback は同期呼び出しのみを既定とする。

非同期 callback は runtime の event loop へ marshal する必要がある。

```lume
ffi callback async LogCallback(message: Utf8String): Void
```

非同期 callback は warning を出す。だいたい複雑になる。人類は callback で何度も文明を燃やしてきた。

---

### 27.18 thread safety

```lume
ffi module fastmath {
  language "c"
  library "./native/libfastmath.so"
  threadSafe true

  fn compute(x: f64): f64
}
```

thread safe でない module。

```lume
ffi module legacy {
  threadSafe false
  lock global

  fn run(): i32
}
```

`lock global` は呼び出しを直列化する。

---

### 27.19 async FFI

長時間実行される FFI は worker thread へ送れる。

```lume
ffi module ocr {
  language "c"
  library "./native/libocr.so"

  async fn recognize(image: Borrowed<Bytes>): Owned<Utf8String> free=string_free
    worker true
}
```

Server Action からの使用。

```lume
server action recognizeText(file: File): String
  runtime "native"
{
  return await ocr.recognize(await file.bytes())
}
```

---

### 27.20 build 定義

FFI module はビルド方法を定義できる。

```lume
ffi module fastmath {
  language "c"
  sources [
    "./native/fastmath.c"
  ]
  includeDirs [
    "./native/include"
  ]
  cflags ["-O3", "-ffast-math"]

  fn sin_fast(x: f64): f64
}
```

C++。

```lume
ffi module codec {
  language "cpp"
  standard "c++20"
  sources ["./native/codec.cpp"]
  cxxflags ["-O3"]
  link ["z", "png"]

  fn decode(input: Borrowed<Bytes>): Owned<Bytes> free=bytes_free
}
```

---

### 27.21 platform 条件 [未実装]

```lume
ffi module nativehash {
  language "c"

  target linux-x64 {
    library "./native/linux-x64/libhash.so"
  }

  target darwin-arm64 {
    library "./native/darwin-arm64/libhash.dylib"
  }

  target windows-x64 {
    library "./native/windows-x64/hash.dll"
  }

  fn hash(input: Borrowed<Bytes>): u64
}
```

---

### 27.22 安全レベル

FFI module は安全性レベルを持つ。

```lume
ffi module fastmath safe {
  fn add(a: i32, b: i32): i32
}
```

```lume
ffi module rawlib unsafe {
  fn dangerous(ptr: Ptr<void>): Void
}
```

`unsafe` module の呼び出しには `unsafe` ブロックが必要である。

```lume
server action runDangerous(): Void
  runtime "native"
{
  unsafe {
    rawlib.dangerous(ptr)
  }
}
```

---

### 27.23 FFI と Server Actions

FFI は Server Action と組み合わせて使うことを想定する。

```lume
ffi module qrcode {
  language "c"
  library "./native/libqrcode.so"

  fn encode(input: Utf8String): Owned<Bytes> free=qrcode_free
  fn qrcode_free(ptr: Ptr<u8>): Void
}

server action createQrCode(text: String): Bytes
  runtime "native"
  auth required
  rateLimit {
    key: auth.user.id
    limit: 30
    window: 1m
  }
{
  return qrcode.encode(text)
}
```

クライアントは FFI を直接呼ばない。

```lume
component QrButton {
  view {
    Button("Generate") {
      on click async {
        const image = await createQrCode("hello")
      }
    }
  }
}
```

---

### 27.24 FFI manifest

コンパイラは FFI manifest を生成する。

```json
{
  "ffi": [
    {
      "name": "qrcode",
      "language": "c",
      "library": "./native/libqrcode.so",
      "runtime": ["native", "node", "bun"],
      "symbols": [
        {
          "name": "encode",
          "params": ["Utf8String"],
          "return": "Owned<Bytes>"
        }
      ]
    }
  ]
}
```

実際の `lume.manifest.json` には `ffi`、`ffiStructs`、`ffiEnums`、`ffiOpaques` が別配列で出力される。`lume.backend.json` には native bridge の解決結果とエラー一覧が入る。

---

### 27.25 FFI バックエンド

実装バックエンド候補。

```txt
native dynamic loading: dlopen / dlsym / LoadLibrary
static linking
LLVM generated bridge function
WASI component model
```

ターゲットごとの推奨。

```txt
native -> static link or dlopen
jit    -> dlopen + cached symbol table, or JIT-generated bridge
wasm   -> WASI component model where available
```

Node.js N-API、Bun FFI、Deno FFI は標準仕様から除外する。外部 adapter としての実装は可能だが、Lume 本体の FFI backend ではない。

---

### 27.26 WASM FFI

C / C++ を WASM にコンパイルして呼ぶ形式も許可する。

```lume
ffi module imagewasm {
  language "c"
  target "wasm32-wasi"
  wasm "./native/image.wasm"

  fn resize(input: Borrowed<Bytes>, width: i32, height: i32): Owned<Bytes> free=free_bytes
}
```

WASM FFI は edge / worker runtime でも利用可能な場合がある。

---

### 27.27 FFI 診断

```txt
LUME3001: FFI type is not allowed in client component
LUME3002: pointer escaped from FFI boundary
LUME3003: owned value missing free function
LUME3004: C++ exception cannot cross FFI boundary
LUME3005: target runtime does not support native FFI
LUME3006: struct layout is ambiguous
LUME3007: unsafe FFI call requires unsafe block
LUME3008: non-thread-safe FFI used concurrently
LUME3009: symbol not found in native library
LUME3010: library is missing for current platform
LUME3011: Int is ambiguous at FFI boundary; use i32 or i64
LUME3012: borrowed value escapes lifetime
```

---

## 28. フォーム

### 28.1 form state

```lume
form login {
  email: String = ""
  password: String = ""
}
```

### 28.2 validation

```lume
form signup {
  email: String = "" validate {
    required
    email
  }

  password: String = "" validate {
    minLength(8)
  }
}
```

### 28.3 使用例

```lume
Form bind=signup {
  Field name="email" label="Email" {
    Input()
  }

  Field name="password" label="Password" {
    Input(type="password")
  }

  Button("Create account", type="submit")
}
```

---
