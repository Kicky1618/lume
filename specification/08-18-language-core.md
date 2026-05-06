## 8. 値とリテラル

この章は、Lume の式と値の最小単位を扱う。文字列、数値、真偽値、配列、オブジェクト、補間、式評価の感覚をここでそろえる。

```txt
literal -> expr -> component props -> view rendering
```

後続の「component」「state」「view」は、この章で定義する値の振る舞いを前提に組み立てられる。

### 8.1 文字列

```lume
"hello"
'hello'
```

文字列補間。

```lume
Text("Hello, {user.name}")
```

### 8.2 数値

```lume
123
3.14
0xff
0b1010
```

### 8.3 真偽値

```lume
true
false
```

### 8.4 null / undefined

```lume
null
undefined
```

### 8.5 配列

```lume
[1, 2, 3]
```

### 8.6 オブジェクト

```lume
{
  id: "1",
  name: "Kicky"
}
```

---

## 9. 式

Lume の式は TypeScript に近い構文を持つ。

### 9.1 算術

```lume
count + 1
width * 2
```

### 9.2 比較

```lume
count > 0
name == "cat"
name != "dog"
```

厳密比較 `===` / `!==` も許可する。

### 9.3 論理演算

```lume
a && b
a || b
!a
```

### 9.4 Null 合体

```lume
user.name ?? "anonymous"
```

### 9.5 Optional chaining

```lume
user.profile?.image
```

### 9.6 三項演算子

```lume
count > 0 ? "positive" : "zero"
```

UI 内では `if` の使用を推奨する。

---

## 10. コンポーネント

### 10.1 基本形

```lume
component Counter {
  state count: Int = 0

  view {
    Column gap=12 {
      Text("Count: {count}")
      Button("Increment") {
        on click {
          count += 1
        }
      }
    }
  }
}
```

### 10.2 Props

```lume
component UserCard(user: User, compact: Bool = false) {
  view {
    Card {
      Text(user.name)
      if !compact {
        Text(user.bio)
      }
    }
  }
}
```

Props はデフォルトで readonly である。

```lume
component Example(value: Int) {
  action bad {
    value += 1 // error
  }
}
```

### 10.3 Children

`slot` を使って子要素を受け取る。

```lume
component Card(title: String) {
  view {
    Box style=card {
      Text(title)
      slot
    }
  }
}
```

使用例。

```lume
Card(title="Info") {
  Text("body")
}
```

### 10.4 Named slot

named slot の fill は `slot:<name> { ... }` 構文のみを許可する。

`header { ... }` のような裸ブロックは通常の子要素と曖昧になるため、v0.1 では禁止する。

```lume
component Page {
  view {
    Header {
      slot header
    }

    Main {
      slot
    }

    Footer {
      slot footer
    }
  }
}
```

使用例。

```lume
Page {
  slot:header {
    Text("Title")
  }

  Text("Main content")

  slot:footer {
    Text("Footer")
  }
}
```

---

## 11. 状態

### 11.1 state

`state` はコンポーネント内で変更可能な値を宣言する。

```lume
state count: Int = 0
```

### 11.2 更新

```lume
count += 1
count = 0
```

配列やオブジェクトの破壊的変更は、コンパイルターゲットに応じて安全な更新へ変換する。

```lume
todos.push(todo)
```

生成 runtime では immutable copy、差分更新、または専用 mutable store のいずれかへ正規化される。

### 11.3 state の制約

`state` は `component` / `page` の直下でのみ宣言できる。

```lume
if condition {
  state x: Int = 0 // error
}
```

---

## 12. derived

`derived` は状態や props から計算される読み取り専用値である。

```lume
derived remaining = count(todos, where done == false)
```

`derived` は依存値が変更されたときに再計算される。

```lume
derived expensive memo = computeLargeValue(items)
```

`memo` を付けた場合はメモ化を要求する。

---

## 13. action

### 13.1 基本形

`action` は UI イベントから呼び出される処理を宣言する。

```lume
action increment {
  count += 1
}
```

### 13.2 引数

```lume
action setName(value: String) {
  name = value
}
```

### 13.3 async action

```lume
async action save {
  await api.post("/save", form)
}
```

### 13.4 戻り値

UI action の戻り値は原則 `Void` とする。

```lume
action compute(): Int {
  return count + 1
}
```

戻り値を持つ action は許可するが、イベントハンドラからの戻り値は無視される。

### 13.5 async モデル

`async action` は 1 つのコンポーネント内で逐次キューとして実行する。

同一 action が多重発火した場合の既定は `enqueue` とする。

```txt
enqueue: 到着順に実行
drop: 実行中なら新規呼び出しを捨てる
restart: 実行中を中断して最新を実行
```

明示指定。

```lume
async action search(query: String) concurrency=restart {
  await fetchSearch(query)
}
```

`await` 境界をまたいだ `View<T>` / `Borrowed<T>` の保持は禁止する。

---

## 14. effect

`effect` は副作用を表す。

```lume
effect [user.id] {
  console.log(user.id)
}
```

依存配列を省略した effect は各レンダー後に実行されるため、警告対象とする。

```lume
effect {
  console.log("rendered") // warning
}
```

cleanup。

```lume
effect [socketUrl] {
  const socket = connect(socketUrl)

  return {
    socket.close()
  }
}
```

SSR 中に effect は実行されない。

---

## 15. view

### 15.1 基本

`view` はコンポーネントが描画する UI ツリーを定義する。

```lume
view {
  Text("Hello")
}
```

1 つの `view` は単一のルートノードを持つことを推奨する。ただし、複数ルートは Fragment に変換される。

```lume
view {
  Text("A")
  Text("B")
}
```

### 15.2 要素呼び出し

```lume
Button("Save", variant="primary")
```

### 15.3 ブロック子要素

```lume
Card {
  Text("Title")
  Text("Body")
}
```

### 15.4 属性

```lume
Box width=100 height=200 hidden=false
```

### 15.5 式属性

```lume
Text(user.name)
Avatar(src=user.image)
```

### 15.6 Boolean 属性

```lume
Input disabled
```

は以下と等価。

```lume
Input disabled=true
```

---

## 16. 条件分岐

### 16.1 if

```lume
if user.loggedIn {
  Dashboard()
} else {
  LoginForm()
}
```

### 16.2 else if

```lume
if status == "loading" {
  Spinner()
} else if status == "error" {
  ErrorView()
} else {
  Content()
}
```

### 16.3 match

```lume
match status {
  case "idle" {
    Text("Idle")
  }
  case "loading" {
    Spinner()
  }
  case "error" {
    ErrorView()
  }
  default {
    Content()
  }
}
```

Union 型に対して `match` が網羅的でない場合、警告する。

---

## 17. 繰り返し

### 17.1 for

```lume
for item in items {
  Text(item.name)
}
```

### 17.2 index

```lume
for item, index in items {
  Text("{index + 1}. {item.name}")
}
```

### 17.3 key

繰り返し要素には `key` が必要である。

```lume
for item in items key=item.id {
  UserCard(user=item)
}
```

`key` がない場合、コンパイラは以下の順に推測する。

1. `item.id`
2. `item.key`
3. index

index を key にする場合は警告する。

---

## 18. イベント

### 18.1 on

```lume
Button("Click") {
  on click {
    count += 1
  }
}
```

### 18.2 イベント引数

```lume
Input(value=name) {
  on input(value) {
    name = value
  }
}
```

### 18.3 DOM イベント

標準 DOM イベントは `click`, `input`, `change`, `submit`, `keydown`, `keyup`, `focus`, `blur` などを持つ。

```lume
Form {
  on submit(event) {
    event.preventDefault()
    save()
  }
}
```

### 18.4 カスタムイベント

```lume
component SearchBox {
  emit select(value: String)

  view {
    Input {
      on input(value) {
        emit select(value)
      }
    }
  }
}
```

使用側。

```lume
SearchBox {
  on select(value) {
    keyword = value
  }
}
```

---
