---
title: Guild
layout: hextra-home
description: "Guild — нативное окно для macOS, чтобы вести много ИИ-агентов сразу: оно создаёт worktree, запускает в них Claude Code, Codex и OpenCode, показывает, кто ждёт вас первым, и отвечает на командную строку, которой пользуются и ваши агенты."
---

{{< landing/hero
  title="Все ваши агенты в одном окне"
  subtitle="Guild создаёт worktree, запускает агентов и показывает, кто ждёт вас первым."
  shot="sessions"
  alt="Окно Guild: в колонке пять сессий агентов по тому, кто ждёт вас первым — ждут, готовы, простаивают, — рядом diff, который предложил агент, с кнопками «Применить» и «Отклонить»"
>}}

{{< landing/cli id="cli" title="Управляйте из командной строки"
  lead="Проекты, рабочие области, worktree и сессии создаются через `guild` так же, как в окне, — вами или агентом в одной из вкладок."
  note="У каждой сессии, которую запускает Guild, `guild` есть в PATH. Агент, который сам получил задачу, запустить другого с задачей не может."
  link="https://github.com/proshik/guild#the-window"
  video="orchestrate"
  alt="Claude Code выполняет guild worktree create и guild agent wait; в колонке появляется worktree readme, и в нём работает Codex"
>}}
{{< landing/cmd run="guild workspace create NAME" >}}
Именованный набор проектов; `guild workspace select` переключает на него.
{{< /landing/cmd >}}
{{< landing/cmd run="guild worktree open PATH" >}}
Репозиторий появляется в колонке вместе со всеми worktree, которые знает git.
{{< /landing/cmd >}}
{{< landing/cmd run="guild worktree create NAME --prompt TEXT" >}}
Новый worktree и агент, запущенный в нём с задачей.
{{< /landing/cmd >}}
{{< landing/cmd run="guild session new --group NAME" >}}
Сессия в папке группы, где её проекты лежат рядом.
{{< /landing/cmd >}}
{{< landing/cmd run="guild agent list" >}}
Все сессии и то, чем занят каждый агент.
{{< /landing/cmd >}}
{{< landing/cmd run="guild agent wait SESSION" >}}
Ждёт, пока агент закончит ход; `guild agent read` возвращает, что он ответил.
{{< /landing/cmd >}}
{{< /landing/cli >}}

{{< landing/phases id="phases" title="От worktree до отправленного коммита"
  lead="Каждый шаг так, как он выглядит в Guild. Выберите любой или смотрите подряд."
>}}
{{< landing/phase id="organize" label="Порядок" icon="folder-tree" video="open"
  title="Проекты, worktree и рабочие области в одной колонке"
  lead="Проект — это репозиторий, а его строки — его worktree, прочитанные из git. Проекты собираются в группы, а наборы проектов — в именованные рабочие области."
  link="https://github.com/proshik/guild#the-window"
  alt="Worktree, созданный через git в терминале, появляется в колонке; его открывают вместе с файлами и main.go"
>}}
{{< landing/detail title="Worktree откуда угодно" >}}
Созданный вручную или агентом в терминале появится так же, как созданный здесь.
{{< /landing/detail >}}
{{< landing/detail title="Группа на папке" >}}
Группу можно привязать к папке с её репозиториями; сессия там видит их все.
{{< /landing/detail >}}
{{< landing/detail title="Удаление с предупреждением" >}}
Перед удалением worktree лист называет всё, что git удалил бы молча.
{{< /landing/detail >}}
{{< /landing/phase >}}
{{< landing/phase id="start" label="Запуск" icon="terminal" video="delegate"
  title="Агент на worktree, во вкладке"
  lead="Запустите Claude Code, Codex или OpenCode в worktree — кнопкой `+` или командой `guild session new`. Каждый работает в своём CLI в терминале Guild, как и везде."
  link="https://github.com/proshik/guild#terminal-sessions-in-the-desktop"
  alt="Codex работает во вкладке worktree: правит tides.go, добавляет тест и запускает go test"
>}}
{{< landing/detail title="Своя команда запуска" >}}
Префикс и аргументы для каждого агента — под обёртку или нужную модель.
{{< /landing/detail >}}
{{< landing/detail title="Области и панели" >}}
Делите экран на области, а вкладку на панели — в каждой свой агент.
{{< /landing/detail >}}
{{< landing/detail title="С того же места" >}}
Закрытый разговор продолжается со стартового экрана или из списка сессий.
{{< /landing/detail >}}
{{< /landing/phase >}}
{{< landing/phase id="follow" label="Внимание" icon="bell" video="follow"
  title="Сразу видно, кто ждёт вас"
  lead="Когда агентов много, переключите колонку на сессии: сначала те, кто ждёт вас, потом работающие, готовые и простаивающие. Вкладка, ждущая ответа, помечена."
  link="https://github.com/proshik/guild/blob/main/CHANGELOG.md"
  alt="Колонка в режиме сессий: два агента начинают работу, один заканчивает, другой ждёт ответа, и его открывают"
>}}
{{< landing/detail title="Со слов агента" >}}
Каждый CLI сам сообщает, работает он, спрашивает или закончил.
{{< /landing/detail >}}
{{< landing/detail title="Непрочитанное" >}}
Законченный ход помечен, пока вы его не посмотрели.
{{< /landing/detail >}}
{{< landing/detail title="Затихшие" >}}
Агенты, простаивающие днями, и закрытые разговоры ждут внизу — их можно продолжить.
{{< /landing/detail >}}
{{< /landing/phase >}}
{{< landing/phase id="review" label="Проверка" icon="eye" video="review"
  title="Прочитайте правку до того, как она попадёт в файл"
  lead="С `claude --ide` правка приходит вкладкой с diff и кнопками «Применить» и «Отклонить». Любое изменение открывается бок о бок, а отдельный hunk можно добавить в индекс."
  link="https://github.com/proshik/guild#the-window"
  alt="Предложенную агентом правку применяют из вкладки с diff; затем один hunk main.go добавляют в индекс из diff бок о бок"
>}}
{{< landing/detail title="Агент ждёт вас" >}}
Отклоните — он пойдёт дальше без записи; примените — он запишет файл сам.
{{< /landing/detail >}}
{{< landing/detail title="Один hunk в индекс" >}}
`+` у изменения добавляет в индекс только его, `−` убирает обратно.
{{< /landing/detail >}}
{{< landing/detail title="Кто это написал" >}}
Аннотирование показывает автора и время каждого куска строк; клик открывает коммит.
{{< /landing/detail >}}
{{< /landing/phase >}}
{{< landing/phase id="ship" label="Отправка" icon="upload" video="ship"
  title="Коммит и push из того же окна"
  lead="Отметьте, что войдёт в коммит, и напишите сообщение. Журнал ищет по всей истории, а «Отправка коммитов» показывает, что вот-вот уйдёт."
  link="https://github.com/proshik/guild#the-window"
  alt="Коммит пишется на панели коммита, появляется наверху журнала и отправляется из окна «Отправка коммитов»"
>}}
{{< landing/detail title="Ветки и stash" >}}
Переключайте, создавайте и удаляйте ветки, применяйте и удаляйте stash — там, где git согласен.
{{< /landing/detail >}}
{{< landing/detail title="Ветка по умолчанию в актуальном виде" >}}
Чистая `main` или `master` перематывается к upstream; проверка каждые пять минут.
{{< /landing/detail >}}
{{< landing/detail title="Каждая запись в консоли" >}}
Каждая команда, которую выполнило окно, и ответ самого git.
{{< /landing/detail >}}
{{< /landing/phase >}}
{{< /landing/phases >}}

{{< landing/catalog id="more" title="И ещё внутри"
  lead="Всё остальное, что умеет окно, по областям — и где это найти."
>}}
{{< landing/area id="agents" name="Агенты и терминал" >}}
{{< landing/entry name="Три агента" where="+ рядом со вкладками" >}}
Claude Code, Codex и OpenCode — каждый в своём CLI, в терминале самого Guild.
{{< /landing/entry >}}
{{< landing/entry name="Статус от агента" where="Метки вкладок и колонки" >}}
Работает, ждёт вас, закончил — и не прочитан, пока вы не посмотрели.
{{< /landing/entry >}}
{{< landing/entry name="Режим сессий" where="Колонка ▸ Сессии" >}}
Все агенты рабочей области по тому, кто ждёт вас первым; в «Затихших» — простаивающие и закрытые.
{{< /landing/entry >}}
{{< landing/entry name="Diff от Claude Code" where="Вкладка с diff" >}}
С `claude --ide` правка приходит как diff с кнопками «Применить» и «Отклонить».
{{< /landing/entry >}}
{{< landing/entry name="`guild` для агентов" where="guild help" >}}
Worktree, сессии и ожидание другого агента — прямо из терминала агента.
{{< /landing/entry >}}
{{< landing/entry name="Своя команда запуска" where="Настройки ▸ Вкладки и агенты" >}}
Префикс и аргументы для каждого агента — под обёртку или нужную модель.
{{< /landing/entry >}}
{{< landing/entry name="Ссылки в терминале" where="⌘-клик" >}}
Путь к файлу открывается во вкладке на нужной строке; по ссылке можно перейти.
{{< /landing/entry >}}
{{< landing/entry name="Поиск в терминале" where="Терминал" >}}
Поиск по истории, копирование при выделении и вставка в скобках (bracketed paste).
{{< /landing/entry >}}
{{< landing/entry name="Продолжить разговор" where="Стартовый экран" >}}
Всё, что говорилось в папке раньше, — можно продолжить.
{{< /landing/entry >}}
{{< /landing/area >}}
{{< landing/area id="editor" name="Редактор и поиск" >}}
{{< landing/entry name="Искать везде" where="⇧⇧" >}}
Файлы, символы и действия в одном окне.
{{< /landing/entry >}}
{{< landing/entry name="Найти в файлах" where="⌘⇧F" >}}
Совпадения над предпросмотром кода вокруг них, на библиотеках ripgrep.
{{< /landing/entry >}}
{{< landing/entry name="Замена в проекте" where="⌘⇧R" >}}
Во всех файлах проекта, из того же поиска.
{{< /landing/entry >}}
{{< landing/entry name="Символы и действия" where="⌥⌘O, ⇧⌘A" >}}
Переход к объявлению — или к любой команде по её названию.
{{< /landing/entry >}}
{{< landing/entry name="Языковые серверы" where="⌥F7" >}}
Определения, использования и проблемы от языковых серверов проекта.
{{< /landing/entry >}}
{{< landing/entry name="Тридцать четыре грамматики" where="Вкладка файла" >}}
Одни и те же цвета во вкладке, в diff, в аннотировании и в файлах коммита.
{{< /landing/entry >}}
{{< landing/entry name="Клавиши для строк" where="⌘D, ⌥⇧↑, ⌘/" >}}
Дублировать, переносить строки, комментировать и расширять выделение через ⌘↑.
{{< /landing/entry >}}
{{< landing/entry name="Сохранение на ходу" where="300 мс" >}}
Запись после последнего нажатия — и никогда поверх изменения, сделанного на диске.
{{< /landing/entry >}}
{{< landing/entry name="Проводник" where="⌘B" >}}
Учитывает `.gitignore`, метки git на каждой строке, файлы перетаскиваются в Finder и из него.
{{< /landing/entry >}}
{{< /landing/area >}}
{{< landing/area id="git" name="Git" >}}
{{< landing/entry name="Панель коммита" where="⌘K" >}}
Отметьте, что входит в коммит, напишите сообщение, закоммитьте через ⌘↩.
{{< /landing/entry >}}
{{< landing/entry name="Один hunk в индекс" where="+ в diff" >}}
Одно изменение уходит в индекс или обратно — само по себе.
{{< /landing/entry >}}
{{< landing/entry name="Diff бок о бок" where="⇧⌘D" >}}
Все изменённые файлы — против последнего коммита, индекса или любого коммита.
{{< /landing/entry >}}
{{< landing/entry name="Журнал" where="Git ▸ Журнал" >}}
Вся история, с фильтрами по ветке, автору, дате или хешу.
{{< /landing/entry >}}
{{< landing/entry name="Аннотирование" where="Аннотировать" >}}
Кто и когда написал каждый кусок строк; клик открывает коммит.
{{< /landing/entry >}}
{{< landing/entry name="Отправка коммитов" where="⇧⌘K" >}}
Что вот-вот уйдёт, файл за файлом, — до того, как уйдёт.
{{< /landing/entry >}}
{{< landing/entry name="Ветки и stash" where="Ветка в заголовке окна" >}}
Переключайте, создавайте и удаляйте ветки; применяйте и удаляйте stash.
{{< /landing/entry >}}
{{< landing/entry name="Ветка по умолчанию" where="Настройки ▸ Проекты" >}}
Чистая `main` или `master` перематывается к upstream; проверка каждые пять минут.
{{< /landing/entry >}}
{{< landing/entry name="Консоль" where="Git ▸ Консоль" >}}
Каждая запись, которую сделало окно, и ответ самого git.
{{< /landing/entry >}}
{{< /landing/area >}}
{{< landing/area id="window" name="Окно и настройки" >}}
{{< landing/entry name="Области и панели" where="⌃⌘⇧→, ⌃⌘⌥→" >}}
Экран делится на области, а вкладка — на панели.
{{< /landing/entry >}}
{{< landing/entry name="Панели инструментов" where="⌘B, ⌘L, ⌘J" >}}
Проводник, коммит, Git и остальное — в доках, между которыми их можно переносить.
{{< /landing/entry >}}
{{< landing/entry name="Рабочие области и группы" where="Колонка" >}}
Именованные наборы проектов и группы, привязанные к папке, где они лежат.
{{< /landing/entry >}}
{{< landing/entry name="Недавние проекты" where="⌥⇧⌘O" >}}
Поиск по полному пути, так что две копии одного репозитория не путаются.
{{< /landing/entry >}}
{{< landing/entry name="Свои горячие клавиши" where="Настройки ▸ Горячие клавиши" >}}
Нажмите новые клавиши — и они работают сразу.
{{< /landing/entry >}}
{{< landing/entry name="Светлая, тёмная и свой оттенок" where="Настройки ▸ Внешний вид" >}}
Как в macOS или по выбору, с оттенком своего цвета.
{{< /landing/entry >}}
{{< landing/entry name="Лимиты ваших планов" where="Заголовок окна" >}}
Сколько осталось в плане Claude Code или Codex.
{{< /landing/entry >}}
{{< landing/entry name="Открыть в" where="Заголовок окна" >}}
Проект в вашем редакторе, терминале или Finder.
{{< /landing/entry >}}
{{< landing/entry name="Обновления" where="Заголовок окна" >}}
Новая ночная сборка появляется кнопкой «Обновить» — с тем, что в ней нового.
{{< /landing/entry >}}
{{< /landing/area >}}
{{< /landing/catalog >}}

{{< landing/bento id="local" title="Локально и ваше"
  lead="Окно на вашем Mac вокруг CLI агентов, которыми вы уже пользуетесь. Серверу в нём делать нечего."
>}}
{{< landing/tile size="lead" title="Собственные CLI агентов" shot="tile-start"
  alt="Стартовый экран worktree в Guild: можно запустить Claude Code, OpenCode, Codex или терминал"
>}}
Claude Code, Codex и OpenCode работают в своих командных строках — с запросами разрешений и
slash-командами — в терминале самого Guild.
{{< /landing/tile >}}
{{< landing/tile size="side" figure="21" title="именованная запись в git, и больше ничего" >}}
`--force`, `--hard` и `-D` в Guild не написаны нигде, а откат сначала спрашивает.
{{< /landing/tile >}}
{{< landing/tile size="side" figure="~/.config/guild" mono="true" title="Без сервера и аккаунта" >}}
Сессии, вкладки и настройки — файлы на вашем Mac.
{{< /landing/tile >}}
{{< landing/tile size="half" title="Ваши настройки не трогаются" >}}
Guild отдаёт свои хуки только тем сессиям, которые запускает сам. Ваш `~/.claude/settings.json`
не меняется.
{{< /landing/tile >}}
{{< landing/tile size="half" title="Нативное, на Rust" >}}
Один Rust-проект на GPUI. Терминал — собственный, на alacritty_terminal.
{{< /landing/tile >}}
{{< /landing/bento >}}

{{< landing/interview id="faq" title="Вопросы"
  link="https://github.com/proshik/guild#readme" linktext="Подробности — в README"
>}}
{{< landing/qa q="Каких агентов запускает Guild?" >}}
Claude Code, Codex и OpenCode, каждого в своём CLI. Статус работает с Claude Code 2.1.295,
Codex 0.163.0 или OpenCode 2.2 и новее.
[Сессии в терминале](https://github.com/proshik/guild#terminal-sessions-in-the-desktop)
{{< /landing/qa >}}
{{< landing/qa q="Может ли агент управлять Guild?" >}}
Да. У каждой сессии, которую запускает Guild, `guild` есть в PATH: агент может создать
worktree, запустить там другого агента с задачей, дождаться его и прочитать ответ. Агент,
который сам получил задачу, другого запустить не может.
[Окно](https://github.com/proshik/guild#the-window)
{{< /landing/qa >}}
{{< landing/qa q="Нужны ли аккаунт или сервер?" >}}
Нет. Сессии, вкладки и настройки — локальные файлы в `~/.config/guild`.
[Запуск](https://github.com/proshik/guild#running-it)
{{< /landing/qa >}}
{{< landing/qa q="Меняет ли Guild настройки моего агента?" >}}
Нет. Его хуки получают только сессии, которые он запускает, через собственный файл настроек;
ваш `~/.claude/settings.json` остаётся как был.
[Правила проекта](https://github.com/proshik/guild/blob/main/AGENTS.md)
{{< /landing/qa >}}
{{< landing/qa q="Может ли он переписать мою историю git?" >}}
Guild пишет в репозиторий только двадцатью одной именованной командой git, и ничем больше.
`--force`, `--hard`, `-D` и `stash pop` не написаны нигде, а откат сначала спрашивает.
[Решения](https://github.com/proshik/guild/blob/main/docs/decisions.md)
{{< /landing/qa >}}
{{< landing/qa q="Как он обновляется?" >}}
Guild выходит ночными сборками. Когда новая готова, в заголовке окна появляется «Обновить» — с
тем, что в ней нового. [Журнал изменений](https://github.com/proshik/guild/blob/main/CHANGELOG.md)
{{< /landing/qa >}}
{{< landing/qa q="Guild — open source?" >}}
Да: Guild распространяется по лицензии [MIT](https://github.com/proshik/guild/blob/main/LICENSE-MIT)
или [Apache 2.0](https://github.com/proshik/guild/blob/main/LICENSE-APACHE) — на ваш выбор.
{{< /landing/qa >}}
{{< landing/qa q="На чём он работает?" >}}
macOS 13 и новее, на Apple silicon.
[Релизы](https://github.com/proshik/guild/releases)
{{< /landing/qa >}}
{{< /landing/interview >}}

{{< landing/close title="Скачать Guild" shot="close"
  alt="Панель коммита с готовым сообщением рядом с журналом Git"
>}}
1. Скачайте Guild со [страницы релизов](https://github.com/proshik/guild/releases) и перенесите
   его в «Программы».
2. Выберите **File ▸ Open…** и укажите репозиторий.
3. Нажмите **+** рядом со вкладками и выберите агента.

Guild пока не нотаризован: в первый раз откройте его из «Программ» через Control-клик ▸ «Открыть».
{{< /landing/close >}}
