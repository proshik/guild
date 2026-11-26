//! Интерфейс по-русски.
//!
//! See [`super::english`] for why a language is a file.

use super::Text;

pub(super) fn russian(text: Text) -> &'static str {
    match text {
        Text::CommitSelectionChanged => "Выбор изменился. Откройте меню заново для текущей группы.",
        Text::CommitRollbackMixed => {
            "Выберите только отслеживаемые файлы без конфликтов. Ничего не изменено."
        }
        Text::CommitAddMixed => {
            "Для добавления в Git выберите только неотслеживаемые файлы. В выборе есть отслеживаемые изменения."
        }
        Text::CommitTrashMissing => {
            "В выборе есть уже удалённые файлы. Для перемещения в корзину выберите существующие файлы."
        }
        Text::CommitDiffDirectories => "В выборе есть каталоги. Разверните их и выберите файлы.",
        Text::CommitDiffUnavailable => "Различия недоступны",
        Text::CommitInclusionUnavailable => "Включение в коммит недоступно",
        Text::CommitRollbackUnavailable => "Откат недоступен",
        Text::CommitAddUnavailable => "Добавление в Git недоступно",
        Text::CommitTrashUnavailable => "Перемещение в корзину недоступно",
        Text::CommitExpandDirectory => "Развернуть каталог",
        Text::CommitCollapseDirectory => "Свернуть каталог",
        Text::CommitSpaceHint => {
            "В списке Пробел включает выбранные файлы в коммит или исключает их."
        }
        Text::CommitDirectoryLoading => "Загрузка…",
        Text::CommitDirectoryEmpty => "Пусто; Git не отслеживает пустые каталоги",
        Text::CommitDirectoryNoEligible => {
            "Нет файлов для коммита; Git не отслеживает пустые каталоги"
        }
        Text::CommitPathNul => "Пути с символом NUL недопустимы",
        Text::CommitUnsupportedPath => "Пути с именами не в UTF-8 не поддерживаются",
        Text::CommitTrashRoot => "Нельзя переместить открытую папку проекта в корзину",
        Text::CommitDirectoryInvalidName => "Некорректное имя каталога",
        Text::CommitRollbackGroupDetail => {
            "Изменения в рабочей копии будут потеряны. Новые файлы будут исключены из индекса и сохранены на диске."
        }

        Text::ProcessOpenJob => "Открыть вывод ревью",
        Text::ProcessJobUnavailable => {
            "Сохранённое живое окно задания недоступно. Ссылка на архив остаётся; это действие не запускает ревью повторно."
        }

        Text::ProcessContext => "Контекст процесса",
        Text::ProcessSelectedStep => "Выбранный шаг",
        Text::ProcessExecuting => "Сейчас выполняется",
        Text::ProcessWorker => "Исполнитель",
        Text::ProcessEvents => "Последние события",
        Text::ProcessNoEvents => "События ещё не записаны",
        Text::ProcessContract => "Контракт",
        Text::ProcessEvidence => "Контракт и доказательства",
        Text::ProcessNoEvidence => "Для этого шага нет сохранённых файлов",
        Text::ProcessRecordedSource => "Зафиксированная версия",
        Text::ProcessResources => "Ресурсы",
        Text::ProcessNoResources => "Для шага не заданы ресурсы скиллов",
        Text::ProcessSessionUnavailable => {
            "Подходящей открытой сессии нет; этот переход не запускает сессию."
        }
        Text::ProcessFileUnavailable => {
            "Файл отсутствует или находится вне доступного исходного проекта. Проверьте сохранённый путь в сессии."
        }
        Text::ProcessControlsUnavailable => {
            "Этот экран показывает записанный ход работы. Назначение задач, повторы, исправления и автоматическое продолжение здесь недоступны; поддерживаемые команды выполняются из исходной сессии."
        }
        Text::ProcessQuestionUnavailable => {
            "Сессия ждёт ответа. Текст вопроса в «Процессе» недоступен; откройте сессию, чтобы его прочитать."
        }
        Text::ProcessWaitingUnavailable => {
            "Сессия ждёт человека. Что ей нужно, в «Процессе» не видно; откройте сессию, чтобы это узнать."
        }
        Text::ProcessPermissionUnavailable => {
            "Сессия ждёт разрешения инструмента. Подробности запроса здесь недоступны; проверьте их в сессии. «Процесс» не выдаёт разрешения."
        }
        Text::ProcessSelectedFallback => {
            "Прежний выбранный шаг недоступен; показан доступный текущий шаг:"
        }
        Text::ProcessStaleSnapshot => {
            "Показан последний сохранённый снимок; последнее обновление не удалось."
        }
        Text::ProcessCloseKeepsRunning => "Закрытие этой вкладки не останавливает работу.",
        Text::ProcessSourceDifferent => {
            "Сохранённое доказательство · другая зафиксированная версия"
        }
        Text::ProcessSourceCurrent => "Зафиксированная версия совпадает со снимком",
        Text::ProcessNoAssignment => "Для этого шага не записана привязанная сессия исполнителя.",
        Text::ProcessRole => "Роль",
        Text::ProcessOutcomes => "Результаты шага",
        Text::ProcessDependencies => "Зависит от",
        Text::ProcessDefinition => "Конфигурация",
        Text::ProcessOpenFile => "Открыть файл",
        Text::ProcessUnrecorded => "Не записано",
        Text::ProcessEventStart => "Процесс запущен",
        Text::ProcessEventCheckpoint => "Проверенный результат записан",
        Text::ProcessEventAssignment => "Назначение наблюдалось",
        Text::ProcessEventDelivery => "Доставка задания наблюдалась",
        Text::ProcessEventRecovery => "Переход / восстановление родителя наблюдалось",
        Text::ProcessEventReport => "Отчёт исполнителя сохранён",
        Text::ProcessEventReview => "Наблюдение ревью сохранено",
        Text::ProcessEventCorrection => "Исправление в той же сессии",
        Text::ProcessCorrectionSubmitted => "Ввод отправлен; начало хода не подтверждено",
        Text::ProcessCorrectionObserved => {
            "Начало хода наблюдалось; нужен отчёт по изменённому source"
        }
        Text::ProcessCorrectionUnknown => {
            "Доставка неопределённа; проверьте receipt, не повторяйте ввод"
        }
        Text::ProcessCorrectionParent => "Нужен checkpoint контракта родителя; ввод не отправлен",
        Text::ProcessCorrectionRefused => "Отказ до ввода; доступен явный повтор",
        Text::ProcessEventCancel => "Отмена запрошена",
        Text::ProcessEventReservation => "Действие зарезервировано; результат не подтверждён",
        Text::ProcessEventUnknown => "Сохранённое событие",
        Text::ProcessLightweight => "Краткий контракт",
        Text::ProcessNoReview => "Для шага нет записи ревью",
        Text::ProcessPreviousResumeWait => {
            "Предыдущая попытка восстановления ещё требует сверки; переход к живой сессии не снимает это ожидание."
        }

        Text::ProcessRecoveryAreaFallback => {
            "Исходная область недоступна; группа открыта в единственной области исходного проекта и рабочего пространства."
        }
        Text::ProcessRecoveryLive => "Открыта исходная сессия; новый процесс не запускался",
        Text::ProcessRecoveryLaunched => {
            "Запрошен запуск того же разговора; готовность не подтверждена"
        }
        Text::ProcessRecoveryGap => "Не удалось восстановить родительскую сессию",
        Text::ProcessParentAttention => "Требуется внимание родителя",
        Text::ProcessReconcileEvent => "Сверка подтверждена родителем",
        Text::ProcessReconcileChecklist => "Сверка после восстановления",
        Text::ProcessReconcileClear => {
            "Факты квалифицированы. Исходный родитель может подтвердить эту оценку через guild workflow reconcile; ввод в harness не отправляется."
        }
        Text::ProcessHandoffReserved => "Prompt квалифицирован; передача зарезервирована до ввода.",
        Text::ProcessHandoffReady => {
            "Исходная беседа восстановлена; доступность prompt подтверждена независимо."
        }
        Text::ProcessHandoffContinued => {
            "После передачи сохранено следующее действие родителя с проверкой исходников."
        }
        Text::ProcessHandoffStartup => "Способ передачи: startup payload при native resume.",
        Text::ProcessHandoffStartupSubmitted => {
            "Сводка передана исходному conversation при native launch; ожидается достоверное событие начала хода. Trust, вход и разрешения остаются отдельными запросами человеку."
        }
        Text::ProcessHandoffTitle => "Передача состояния родителю",
        Text::ProcessHandoffWaiting => {
            "Ожидается подтверждённое завершение turn и доступный пустой prompt исходной сессии."
        }
        Text::ProcessHandoffBusy => {
            "Исходная сессия работает или ожидает вопрос/разрешение. Ввод не отправлен."
        }
        Text::ProcessHandoffDraft => {
            "Автоматическое продолжение приостановлено: обнаружен неотправленный ввод. Guild не отправлял и не менял его. Завершите, отправьте или удалите черновик в исходной сессии; остальные проверки готовности сохраняются."
        }
        Text::ProcessHandoffInputUsed => {
            "После восстановления в терминал поступил ввод. Автоматическая передача недоступна; откройте исходную сессию."
        }
        Text::ProcessHandoffInputUnknown => {
            "Эта версия OpenCode не подтверждает готовность пустого поля ввода. Автоматическая сводка не отправлена; откройте исходную сессию. Наличие черновика неизвестно."
        }
        Text::ProcessHandoffHolder => {
            "Исходный процесс недоступен или изменился; передача в заменивший его процесс запрещена."
        }
        Text::ProcessHandoffCovered => {
            "Prompt занят оверлеем или другой отправкой; передача ожидает."
        }
        Text::ProcessHandoffTransport => {
            "Harness не подтвердил возможность отправки в prompt. Откройте исходную сессию; ввод при запуске не подставляется."
        }
        Text::ProcessHandoffSnapshot => {
            "Перед передачей нужны актуальные исходники, контракт и данные процесса."
        }
        Text::ProcessHandoffSubmitted => {
            "Сводка отправлена через принадлежащий сессии терминал; ожидается подтверждение начала turn. Это ещё не сверка и не продолжение работы."
        }
        Text::ProcessHandoffAcknowledged => {
            "Начало turn после передачи и явная сверка родителя сохранены. Продолжайте через следующий допустимый настроенный checkpoint."
        }
        Text::ProcessHandoffObserved => {
            "Начало turn после передачи подтверждено. Исходный родитель должен проверить и явно подтвердить свежую оценку сверки."
        }
        Text::ProcessHandoffUnknown => {
            "Доставка сводки неизвестна. Проверьте исходную сессию и сохранённую запись; автоматический повтор запрещён."
        }
        Text::ProcessReconcileRecorded => {
            "Сверка подтверждена родителем. Подтверждение не отправляет ввод; доставка сводки учитывается отдельно."
        }
        Text::ProcessReconcileStale => {
            "Прежнее подтверждение устарело; проверьте и подтвердите текущие факты заново."
        }
        Text::ProcessReconcileSource => {
            "Исходники изменились: сначала передайте source-qualified evidence из исходной родительской сессии."
        }
        Text::ProcessReconcileContract => {
            "Зафиксированный контракт отсутствует или изменился; проверьте сохранённые доказательства."
        }
        Text::ProcessReconcileOrigin => {
            "Владение исходным разговором или runtime не подтверждено; проверьте исходную сессию."
        }
        Text::ProcessReconcileHolder => {
            "Владелец или identity назначенного runtime неизвестны; проверьте сессию без запуска замены."
        }
        Text::ProcessReconcilePermission => {
            "Разрешение инструмента не отвечено. Откройте указанную сессию; Process не отвечает за пользователя."
        }
        Text::ProcessReconcileQuestion => {
            "Ожидается ответ человека. Откройте указанную сессию; подробности вопроса здесь недоступны."
        }
        Text::ProcessReconcileEffect => {
            "У effect reservation нет подтверждённого результата. Сверьте фактического владельца; не повторяйте действие."
        }
        Text::ProcessReconcileDelivery => {
            "Доставка задания child не подтверждена; проверьте сохранённое назначение и сессию."
        }
        Text::ProcessReconcileReport => {
            "У остановленного child нет source-qualified отчёта; перед продолжением получите сохранённый результат."
        }
        Text::ProcessReconcileChildSource => {
            "Исходники или отчёт child устарели либо недоступны; проверьте его checkout и evidence."
        }
        Text::ProcessReconcileReview => {
            "Выполнение ревью или фактический расход ожидаются либо неизвестны; сверьте существующие job/archive без нового запуска."
        }
        Text::ProcessReconcileResource => {
            "Владелец записанного ресурса или private environment сохранён, изменился либо неизвестен; сверьте именно этого владельца."
        }
        Text::ProcessReconcileUnknown => {
            "Текущая сверка недоступна; проверьте исходную сессию и сохранённую историю."
        }
        Text::ProcessReconcileStatus => {
            "Native status ещё не наблюдался; дождитесь квалифицированного события именно этой сессии."
        }
        Text::ProcessRecover => "Продолжить родительскую сессию и открыть процесс",
        Text::ProcessRecoveryPending => {
            "Восстановление зарезервировано; запуск и готовность не подтверждены"
        }
        Text::ProcessRecoveryWait => {
            "Запуск терминала не означает готовность или продолжение оркестрации. Перед продолжением проверьте дочерние сессии, исходники, расход ревью, ожидающие вопросы и ресурсы. Задание и ответы на разрешения не отправлялись."
        }
        Text::ProcessOpen => "Открыть процесс",
        Text::ProcessMoveUnavailable => {
            "Связь процесса недоступна; откройте сохранённый процесс перед перемещением."
        }
        Text::ProcessMoveSelectPane => "Выберите родительскую панель или Process нужной группы.",
        Text::ProcessMoveSeat => {
            "Связанные виды находятся в другом проекте или workspace; сначала откройте исходную группу."
        }
        Text::HarnessFailureNetwork => {
            "Проверьте подключение к интернету, VPN или прокси и повторите попытку."
        }
        Text::HarnessFailureDetails => {
            "Подробности — в выводе терминала ниже. Затем повторите попытку."
        }
        Text::HarnessRetry => "Повторить",
        Text::HarnessRetryUnknown => {
            "Не удалось определить прежний диалог. Начать новый? Предыдущий запрос не будет отправлен повторно."
        }
        Text::HarnessRetryManaged => {
            "Не удалось безопасно определить диалог задачи. Задание и вывод терминала сохранены."
        }
        Text::HarnessRetryOccupied => {
            "Этот ID Claude занят, но возобновляемого диалога нет. Сессия и вывод сохранены."
        }
        Text::HarnessRetryDirectory => {
            "Исходная рабочая папка недоступна. Восстановите её перед повтором."
        }
        Text::HarnessRetryIdentity => {
            "Не удалось безопасно определить диалог. Сессия и вывод терминала сохранены."
        }
        Text::HarnessRetryHeld => {
            "Безопасный повтор невозможен: диалог занят другим процессом или его состояние неизвестно."
        }
        Text::Process => "Процесс",
        Text::ProcessRevision => "ревизия",
        Text::ProcessUnavailable => "Не удалось прочитать запуск",
        Text::ProcessLoading => "Загрузка сохранённого запуска…",
        Text::ProcessReviewUsage => "Бюджет ревью",
        Text::ProcessSteps => "Шаги процесса",
        Text::ProcessOrigin => "Родительская сессия",
        Text::ProcessOpenSession => "Открыть сессию",
        Text::ProcessTarget => "Проект",
        Text::ProcessAssignments => "Исполнители",
        Text::ProcessCheckout => "Рабочее дерево",
        Text::ProcessSession => "Сессия",
        Text::ProcessArchive => "Архив ревью",
        Text::ProcessNoJob => "Запись задания ещё не получена",
        Text::ProcessBlocked => "Заблокирован",
        Text::ProcessReady => "Готов к запуску",
        Text::ProcessActive => "Выполняется",
        Text::ProcessWaiting => "Ожидает",
        Text::ProcessFinished => "Завершён",
        Text::ProcessSkipped => "Пропущен",
        Text::ProcessCancelled => "Отменён",
        Text::ProcessUnknown => "Неизвестно",
        Text::NetworkProxyRoute => "Через прокси",
        Text::NetworkDirectRoute => "Напрямую",
        Text::NetworkSection => "Сеть",
        Text::NetworkIntro => "Настройте подключение Guild и запускаемых им процессов к интернету.",
        Text::NetworkConnection => "Подключение",
        Text::NetworkApply => "Использовать прокси для",
        Text::NetworkHarnesses => "Подключения агентов",
        Text::NetworkHarnessHint => {
            "Для новых сессий: «Как у всех» использует настройку «Сессии агентов» выше. «Прокси» берёт сохранённый адрес и исключения. «Напрямую» очищает прокси-переменные. Явный префикс запуска может это переопределить."
        }
        Text::NetworkFollow => "Как у всех",
        Text::NetworkUseProxy => "Прокси",
        Text::NetworkDirect => "Напрямую",
        Text::NetworkMode => "Режим подключения",
        Text::NetworkModeHint => {
            "Системный режим сохраняет текущие настройки. Свой прокси применяется к выбранным областям."
        }
        Text::NetworkAddress => "Адрес прокси",
        Text::NetworkAddressHint => "HTTP-прокси без авторизации. Доступно в режиме «Свой прокси».",
        Text::NetworkBypass => "Не использовать прокси",
        Text::NetworkBypassHint => {
            "Localhost всегда без прокси. Добавьте адреса или внутренние домены через запятую."
        }
        Text::NetworkGuild => "Запросы Guild",
        Text::NetworkGuildHint => {
            "Лимиты, проверка и загрузка обновлений, внешние изображения Markdown, статус merge request'ов."
        }
        Text::NetworkAgent => "Сессии агентов",
        Text::NetworkAgentHint => "Передавать настройки новым агентам, запускаемым Guild.",
        Text::NetworkTerminal => "Сессии терминала",
        Text::NetworkTerminalHint => "Передавать настройки обычным терминалам Guild.",
        Text::NetworkGit => "Git через HTTPS",
        Text::NetworkGitHint => "Клонирование, fetch и push. SSH настраивается отдельно.",
        Text::NetworkTools => "Инструменты проекта",
        Text::NetworkToolsHint => {
            "Передавать окружение инструментам Guild. Инструмент должен поддерживать proxy-переменные."
        }
        Text::NetworkSystem => "Системный / окружение",
        Text::NetworkCustom => "Свой прокси",
        Text::NetworkCheck => "Проверить подключение",
        Text::NetworkChecking => "Проверяем…",
        Text::NetworkTiming => {
            "Изменения применяются к новым сессиям. Выключение сохраняет прежние настройки."
        }
        Text::NetworkCheckHint => "Проверить доступ к Anthropic через это подключение.",
        Text::NetworkConnected => "Подключение установлено",
        Text::NetworkReachable => "Сервер доступен",
        Text::NetworkEndpointError => "Сервер вернул ошибку или перенаправление",
        Text::NetworkNotUsage => "Доступ к лимитам с авторизацией не проверялся.",
        Text::NetworkSystemCredentials => {
            "Проверка не передаёт авторизацию системного прокси. Системный режим не меняется. Для проверки выберите свой HTTP-прокси без авторизации."
        }
        Text::NetworkSystemTransport => {
            "Проверка не поддерживает этот тип системного прокси. Системный режим не меняется. Для проверки выберите свой HTTP-прокси без авторизации."
        }
        Text::NetworkAuthError => "Прокси требует авторизацию, она не поддерживается.",
        Text::NetworkTimeout => "Время ожидания истекло. Проверьте адрес прокси и сеть.",
        Text::NetworkDnsError => "Не удалось определить адрес прокси или сервера.",
        Text::NetworkRefused => "Не удалось подключиться. Проверьте, что прокси запущен.",
        Text::NetworkTlsError => "Ошибка TLS-подключения или проверки сертификата.",
        Text::NetworkUnavailable => "Проверка недоступна: не удалось запустить системный curl.",
        Text::NetworkInvalid => {
            "Укажите корректный HTTP-адрес и порт без авторизации, а также адреса/домены обхода."
        }
        Text::NetworkFailed => "Ошибка подключения. Проверьте прокси и настройки сети.",

        Text::LspSettings => "Языковые серверы",
        Text::LspDisabled => "Отключён",
        Text::LspEnabled => "Включён",
        Text::LspInherit => "Наследовать",
        Text::LspOn => "Вкл.",
        Text::LspOff => "Выкл.",
        Text::LspNotStarted => "Не запущен",
        Text::LspNotInstalled => "Не установлен",
        Text::LspUnprepared => "Проект не подготовлен",
        Text::LspFailed => "Ошибка",
        Text::LspRunning => "Работает",
        Text::ScopedRepository => "Репозиторий",
        Text::ScopedGroup => "Группа",
        Text::ScopedWorkspaces => "Рабочие области",
        Text::ScopedWorkspace => "Рабочая область",
        Text::ScopedNameHint => "Имя в заголовке и списке рабочих пространств.",
        Text::ScopedIconHint => "Символ рядом с рабочим пространством или репозиторием.",
        Text::ScopedBaseHint => {
            "Ветка для создания worktree. Пустое поле использует глобальную настройку."
        }
        Text::ScopedDirectoryHint => {
            "Каталог будущих worktree. Пустое поле использует автоматическое размещение."
        }
        Text::ScopedDeleteHint => "Перед удалением рабочего пространства появится подтверждение.",
        Text::ScopedSaveHint => "Поля сохраняются по Enter или при переходе к другому элементу.",
        Text::ScopedName => "Название",
        Text::ScopedIcon => "Значок",
        Text::ScopedProjects => "Подключённые проекты",
        Text::ScopedMainPath => "Основной каталог",
        Text::ScopedBase => "База новых worktree",
        Text::ScopedDirectory => "Каталог worktree",
        Text::ScopedOpenSettings => "Открыть настройки",
        Text::ScopedShared => "Общие для всех рабочих областей и worktree",
        Text::ScopedDefaults => "Глобальные значения для наследующих репозиториев",
        Text::ScopedIdentityPending => "Определяем репозиторий…",
        Text::ScopedUnavailable => "Репозиторий недоступен",
        Text::ScopedSave => "Сохранить",
        Text::ScopedRetry => "Повторить",
        Text::ScopedChoose => "Выбрать…",
        Text::ScopedAutomatic => "Автоматически",
        Text::ScopedNoMain => "Основной каталог недоступен",
        Text::ScopedFutureOnly => "Для будущих worktree; существующие каталоги не перемещаются",
        Text::ScopedDeleteWorkspace => "Удалить рабочую область…",
        Text::TerminalUnknownCwd => "Рабочая папка программы неизвестна",
        Text::TerminalThisSession => "Эта сессия",
        Text::TerminalThisWorktree => "Этот worktree",
        Text::TerminalThisProject => "Этот проект",
        Text::TerminalAllLaunches => "Все запуски",
        Text::TerminalOngoing => "Работают",
        Text::TerminalCompleted => "Завершены",
        Text::TerminalScopeEmpty => "В этой области нет запусков",
        Text::TerminalNoSession => "Нет выбранной сессии",
        Text::TerminalNoRoot => "Нет выбранного worktree",
        Text::TerminalUnknownSession => "Исходная сессия неизвестна",
        Text::TerminalUnknownRoot => "Исходный worktree неизвестен",
        Text::TerminalUnknownRepository => "Исходный репозиторий неизвестен",
        Text::TerminalObservedBranch => "Последняя известная ветка",
        Text::TerminalUnknownBranch => "Наблюдение ветки недоступно",
        Text::TerminalProgramCwd => "Рабочая папка программы",
        Text::TerminalAllContexts => "Все проекты и сессии",
        Text::TerminalContextProject => "Проект",
        Text::TerminalContextWorktree => "Рабочее дерево",
        Text::TerminalContextSession => "Сессия",
        Text::TerminalHideView => "Скрыть окно — программа продолжит работать",
        Text::TerminalOpeningArchive => "Открывается архив",
        Text::TerminalViewingArchive => "Просмотр архива",
        Text::TerminalClosingArchive => "Закрывается просмотр",
        Text::TerminalCloseViewer => "Закрыть просмотр",

        Text::TerminalJobs => "Задания терминала",
        Text::TerminalNewest => "Сначала новые",
        Text::TerminalNoJobs => "Нет заданий терминала",
        Text::TerminalUnknownTime => "Время запуска неизвестно",
        Text::TerminalOpenRun => "Открыть прогон",
        Text::TerminalShowOutput => "Показать вывод",
        Text::TerminalDeleteOutput => "Удалить сохранённый вывод",
        Text::TerminalJobDetails => "Исходные сведения о запуске",
        Text::ReviewDetails => "Подробности находок",
        Text::ReviewAnalyze => "Анализ",
        Text::ReviewConsolidate => "Объединение",
        Text::ReviewVerify => "Проверка",
        Text::ReviewAnalyzeProgress => "Анализ кода и сбор кандидатов",
        Text::ReviewConsolidateProgress => "Объединение пересекающихся кандидатов",
        Text::ReviewVerifyProgress => "Проверка собранных кандидатов",
        Text::ReviewSerious => "Серьёзные",
        Text::ReviewWarnings => "Предупреждения",
        Text::ReviewUnknown => "Без категории",
        Text::ReviewProvisional => "Предварительно",
        Text::ReviewAssessed => "Проверено",
        Text::ReviewFinal => "Итог",
        Text::ReviewClean => "Завершено · замечаний нет",
        Text::ReviewIncomplete => "Неполные данные ревью",
        Text::ReviewUnavailable => "Данные ревью недоступны",
        Text::ReviewStale => "Прошлое наблюдение · источник недоступен",
        Text::ReviewLegacy => "Старый архив · полнота неизвестна",
        Text::ReviewOpenQuestion => "Открытый вопрос",
        Text::ReviewPreExisting => "Существовавшая проблема",
        Text::ReviewImmaterial => "Несущественное замечание",
        Text::ReviewRejected => "Отклонено",
        Text::ReviewOther => "Другие категории",
        Text::ReviewDropped => "Отброшено при объединении",
        Text::ReviewReported => "Сообщённая активность (дословно)",
        Text::ReviewNoFindingsYet => "Структурированных находок пока нет",
        Text::TerminalNoSavedOutput => "Нет сохранённого вывода",
        Text::TerminalOutputDeleted => "Сохранённый вывод удалён",
        Text::TerminalOutputEmpty => "Нет сохранённого текста. Программа могла очистить экран.",
        Text::TerminalOutputUnavailable => "Вывод недоступен в этом сеансе Guild",
        Text::TerminalStarting => "Запускается",
        Text::TerminalRunning => "Выполняется",
        Text::TerminalCancelling => "Отменяется",
        Text::TerminalFailed => "Не удалось запустить",
        Text::TerminalInterrupted => "Прервано",
        Text::TerminalExited => "Завершено",
        Text::TerminalSignal => "Сигнал",
        Text::TerminalSelectPane => "Сначала выберите открытую панель",

        Text::GetFromVersionControl => "Получить из системы контроля версий…",
        Text::CloneTitle => "Получить из системы контроля версий",
        Text::UpdaterCleanupRecovery => {
            "Новая сборка запустилась, но очистка обновления была прервана. Закройте Guild перед завершением очистки с сохранением текущей сборки."
        }
        Text::UpdaterJournalWaiting => "Завершается предыдущее обновление…",
        Text::UpdaterRecoveryHelp => {
            "Закройте Guild и выполните эту команду в Терминале. Не открывайте эти копии приложения до её завершения."
        }
        Text::UpdaterCopyRecovery => "Скопировать команду восстановления",
        Text::UpdaterDestination => "Приложение",
        Text::UpdaterLegacyReplacement => {
            "Этот старый updater поддерживает только Local из стандартного каталога и сборки с тегом Nightly. Настройки Remote, канала и каталога сохранятся для возврата в новый Guild; старая версия их не использует."
        }
        Text::UpdaterPrevious => "Путь восстановления прежнего приложения",
        Text::UpdatesSection => "Обновления",
        Text::UpdatesIntro => {
            "Проверяйте новые версии Guild. Скачанное обновление ждёт, пока вы решите перезапустить приложение."
        }
        Text::UpdaterSource => "Источник обновлений",
        Text::UpdaterSourceHint => {
            "Remote — опубликованные релизы. Local — готовые сборки на этом Mac."
        }
        Text::UpdaterChannel => "Канал релизов",
        Text::UpdaterChannelHint => {
            "Nightly — последние изменения. Stable — стабильные релизы, когда они доступны."
        }
        Text::UpdaterDirectory => "Каталог локальных сборок",
        Text::UpdaterDirectoryHint => {
            "Каталог с готовым Guild.app. Пустое поле — стандартный каталог сборок."
        }
        Text::UpdaterStatus => "Состояние обновления",
        Text::UpdaterStatusHint => "Проверка, загрузка и перезапуск используют выбранный источник.",
        Text::UpdaterAbsolutePath => "Укажите абсолютный путь к каталогу.",
        Text::UpdaterDefaultDirectory => "Стандартный каталог сборок",
        Text::UpdaterPublicHelp => {
            "GitHub Releases · proshik/guild. Публичные релизы доступны без авторизации. Для приватного репозитория установите gh и выполните: gh auth login --hostname github.com"
        }
        Text::UpdaterLoading => "Загружаются настройки обновлений…",
        Text::UpdaterSaving => "Сохраняются настройки обновлений…",
        Text::UpdaterSaveFailed => {
            "Не удалось прочитать или сохранить настройки. Повторите перед обновлением."
        }
        Text::UpdaterPreferenceRecovery => {
            "Изменение настроек было прервано. Выберите, какой вариант сохранить."
        }
        Text::UpdaterKeepRequested => "Сохранить новый выбор",
        Text::UpdaterKeepPrevious => "Сохранить прежний выбор",
        Text::UpdaterCheck => "Проверить обновления",
        Text::UpdaterChecking => "Проверяется выбранный источник…",
        Text::UpdaterDownload => "Загрузить обновление",
        Text::UpdaterRestart => "Установить и перезапустить…",
        Text::UpdaterReinstall => "Переустановить…",
        Text::UpdaterInstalling => "Подготовка обновления и перезапуска…",
        Text::UpdaterVerifying => "Проверка сборки…",
        Text::UpdaterEmpty => "В выбранном источнике или канале нет подходящей сборки.",
        Text::UpdaterCurrentVersion => "Текущая версия",
        Text::UpdaterInstalled => "Установлена",
        Text::UpdaterNoNewer => "Более новой ревизии нет.",
        Text::UpdaterLatest => "Последний проверенный релиз",
        Text::UpdaterDetails => "Дополнительные настройки и сведения",
        Text::UpdaterCandidate => "Выбранная сборка",
        Text::UpdaterHistory => "Доступные релизы",
        Text::UpdaterNotes => "Изменения",
        Text::UpdaterUnverified => {
            "Некоторые опубликованные релизы нельзя установить: подписанные метаданные отсутствуют, недействительны или не поддерживаются."
        }
        Text::UpdaterCacheWarning => {
            "Проверенная загрузка готова, но её кеш для следующего запуска не сохранился."
        }
        Text::UpdaterAccessError => {
            "Репозиторий приватный, недоступен или не найден. Проверьте авторизованный доступ gh и повторите."
        }
        Text::UpdaterNetworkError => {
            "Источник обновлений недоступен. Проверьте соединение и повторите."
        }
        Text::UpdaterRateError => {
            "Достигнут лимит запросов GitHub. Повторите после указанного ниже времени сброса."
        }
        Text::UpdaterVerificationError => {
            "Сборка не прошла проверку. Установленное приложение не заменено."
        }
        Text::UpdaterStorageError => {
            "Не удалось подготовить файлы обновления. Проверьте свободное место и доступ к каталогам."
        }
        Text::UpdaterRecoveryError => {
            "Незавершённое обновление требует ручного восстановления. Закройте затронутые копии Guild перед продолжением."
        }
        Text::UpdaterLocalProducer => "Локальная сборка",
        Text::UpdaterCiProducer => "Сборка GitHub Actions",
        Text::UpdaterLegacyInfo => {
            "Старая сборка: идентификатор артефакта и производитель неизвестны."
        }
        Text::CloneDirectoryHelp => "По этому пути будет создана новая папка.",
        Text::CloneFailureHelp => {
            "Git завершился до окончания клонирования. Раскройте вывод Git, чтобы узнать причину."
        }
        Text::CloneUrl => "URL репозитория",
        Text::CloneDirectory => "Каталог",
        Text::CloneChoose => "Выбрать…",
        Text::CloneStart => "Клонировать",
        Text::CloneRunning => "Клонирование репозитория…",
        Text::CloneCancelling => "Отмена…",
        Text::CloneCancelled => "Клонирование отменено. Можно повторить.",
        Text::CloneFailed => "Не удалось клонировать репозиторий",
        Text::CloneHelp => "HTTPS или SSH",
        Text::CloneDetails => "Вывод Git",
        Text::CloneHide => "Продолжить в фоне",
        Text::CloneRetained => "Сохранённый каталог:",
        Text::CloneOpen => "Открыть проект",
        Text::CloneOpenMissing => {
            "Репозиторий готов. Исходное рабочее пространство или каталог недоступны. Откройте его в текущем пространстве или через Файл → Открыть проект."
        }
        Text::CloneComplete => "Репозиторий клонирован.",

        Text::SettingsPreview => "Образец шрифта",
        Text::SettingsLanguage => "Язык",
        Text::SettingsLocalFiles => "Локальные файлы",
        Text::SettingsColourTheme => "Цветовая тема",
        Text::SettingsActionBar => "Панель действий",
        Text::ActionBarItemOrder => "Элементы и порядок",
        Text::ActionBarHint => {
            "Перетаскивайте элементы, удаляйте через × и возвращайте через «Добавить элемент». Tab выбирает элемент, Option+←/→ перемещает его. Прокручивайте вбок. Изменения применяются сразу."
        }
        Text::ActionBarUsage => "Использование",
        Text::ActionBarReset => "Восстановить по умолчанию",
        Text::ActionBarAdd => "Добавить элемент",
        Text::ActionBarRemove => "Удалить",
        Text::ActionBarEmpty => {
            "Нет элементов. Добавьте элемент или восстановите настройки по умолчанию."
        }
        Text::ActionBarAllShown => "Все доступные элементы уже показаны.",
        Text::ActionBarAddHint => "Добавить исключённый элемент в конец панели действий.",
        Text::ActionBarKeyboardHint => "Option+←/→ перемещает этот элемент.",
        Text::SettingsTypography => "Шрифты",
        Text::SettingsSelection => "Выделение",
        Text::LaunchPrefixes => "Команда запуска",
        Text::LaunchPrefixLabel => "Префикс (перед программой)",
        Text::LaunchArgumentsLabel => "Аргументы (после программы)",
        Text::LaunchPrefixHint => {
            "Префикс и аргументы применяются к будущим запускам. Кавычки учитываются, переменные не подставляются. Затем идут флаги Guild."
        }
        Text::SettingsNewSessions => "Новые вкладки",
        Text::SettingsSessionList => "Список сессий",
        Text::SettleAfterTitle => "Сессии затихают через",
        Text::SettleAfterHint => {
            "В режиме Sessions колонки вкладки без активности дольше этого срока уходят в «Затихшие», рядом с закрытыми беседами, в которые агент писал за этот срок."
        }
        Text::SettleAfterUnit => "дн.",
        Text::SettingsClaudePermissions => "Разрешения Claude Code",
        Text::SettingsStartup => "При запуске",
        Text::SettingsClosing => "Закрытие проектов и вкладок",
        Text::SettingsSaving => "Сохранение",
        Text::SettingsDiffs => "Сравнение",
        Text::SettingsSearch => "Поиск настроек…",
        Text::SettingsResults => "Результаты поиска",
        Text::SettingsNoResults => "Настройки не найдены",
        Text::SettingsTrySearch => "Попробуйте другое слово или очистите поиск.",
        Text::SettingsClear => "Очистить поиск",
        Text::SettingsScope => "Личные настройки · Все проекты",
        Text::SettingsImmediate => "Изменения сохраняются автоматически.",
        Text::SettingsModified => "Только изменённые",
        Text::SettingsShortcuts => "Сочетания клавиш",
        Text::SettingsGeneralIntro => "Язык и локальные файлы.",
        Text::SettingsAppearanceIntro => "Удобное для чтения рабочее пространство.",
        Text::SettingsTerminalIntro => "Текст и выделение во вкладках терминала.",
        Text::SettingsSessionsIntro => {
            "Выберите, что открывается в новых вкладках и как запускается каждый агент."
        }
        Text::SettingsProjectsIntro => "Поведение при открытии и закрытии работы.",
        Text::SettingsEditorIntro => "Сохранение файлов и просмотр изменений.",
        Text::SettingsKeymapIntro => {
            "Выберите сочетание, чтобы записать новое. Escape отменяет запись."
        }
        Text::SettingsResultsIntro => "Совпадения во всех категориях.",

        Text::NewSession => "+ сессия",
        Text::NewTerminal => "+ терминал",
        Text::Session => "Сессия",
        Text::Terminal => "терминал",
        Text::Shell => "оболочка",
        Text::Settings => "Настройки",
        Text::Open => "открыта",
        Text::Reopen => "вернуться",
        Text::NoConversation => "без разговора",
        Text::NoProjects => "Проект не открыт.",
        Text::HintNewTerminal => "Новый терминал",
        Text::HintRecentProjects => "Недавние проекты",
        Text::HintOpenProject => "Открыть проект",
        Text::HintNewProject => "Новый проект",
        Text::HintSettings => "Настройки",
        Text::StartChoose => "выберите, что запустить",
        Text::StartAllTools => "Все инструменты",
        Text::StartRecent => "Недавние",
        Text::StartAll => "Все",
        Text::StartDefault => "По умолчанию",
        Text::StartTerminalName => "Терминал",
        Text::StartNotOnPath => "нет в PATH",
        Text::StartResume => "Продолжить",
        Text::StartNoSessions => "В этой ворктри ещё не было сессий.",
        Text::StartSettings => "Настройки",
        Text::StartNewSession => "Новая сессия",
        Text::NoProject => "Нет проекта",
        Text::LanguagePlain => "Обычный текст",
        Text::ChooseConversation => "Выбрать беседу",
        Text::ClaudeWithoutConversation => {
            "Идентификатор беседы не сохранён. Продолжите существующую беседу с начального экрана."
        }
        Text::QuitTitle => "Выйти, пока работают сессии?",
        Text::QuitAnyway => "Выйти",
        Text::Cancel => "Отмена",
        Text::SettingsTitle => "Настройки",
        Text::SectionGeneral => "Общие",
        Text::SectionTerminal => "Терминал",
        Text::SectionSessions => "Вкладки и агенты",
        Text::SectionEditor => "Редактор",
        Text::AutosaveTitle => "Автосохранение",
        Text::AutosaveHint => {
            "Запись через 300 мс после последней клавиши, при потере фокуса и перед коммитом; никогда поверх файла, изменённого на диске"
        }
        Text::FileNotSaved => "Не сохранён",
        Text::SectionExperimental => "Эксперименты",
        Text::SettingsExperimentalIntro => "Пробуйте новые возможности, когда будете готовы.",
        Text::DevelopmentWorkflowsTitle => "Процессы разработки",
        Text::DevelopmentWorkflowsHint => {
            "Включите новые процессы разработки. Уже начатые процессы продолжат работать после выключения."
        }
        Text::SectionKeymap => "Горячие клавиши",
        Text::SettingsDone => "Готово",
        Text::KeymapRecording => "нажми сочетание…",
        Text::KeymapReset => "Сбросить",
        Text::KeymapTaken => "уже занято действием",
        Text::KeymapHint => {
            "Нажми на сочетание, чтобы изменить его. Отличия от значений по \
             умолчанию хранятся в файле; испорченный файл ничего не меняет — \
             остаются значения по умолчанию."
        }
        Text::KeymapFileTitle => "Файл раскладки",
        Text::RevealFile => "Показать",
        Text::TabLauncherOrderTitle => "Порядок запуска вкладок",
        Text::TabLauncherOrderHint => {
            "Удерживайте ⌘⌃, выбирайте ↑/↓ и отпустите клавиши для запуска. Enter или номер строки запускает сразу; Escape отменяет. Порядок здесь и в меню одинаковый."
        }
        Text::DefaultTabTitle => "Вкладка по умолчанию",
        Text::DefaultTabHint => {
            "Что открывает + в полосе вкладок: харнесс или обычный терминал. Стрелка рядом всё ещё предлагает любую вкладку."
        }
        Text::SettingsRendering => "Отрисовка",
        Text::AdaptiveRefreshTitle => "Адаптивная отрисовка до 120 Гц",
        Text::AdaptiveRefreshHint => {
            "Экспериментально. До 120 Гц на поддерживаемом экране; может повысить расход энергии при работе."
        }
        Text::CopyOnSelectTitle => "Копировать при выделении",
        Text::CopyOnSelectHint => {
            "Выделенный текст сразу попадает в буфер обмена. Для новых терминалов."
        }
        Text::FontSizeTitle => "Кегль терминала",
        Text::FontSizeHint => "Cmd+= и Cmd+− меняют его на лету.",
        Text::PermissionsTitle => "Разрешения",
        Text::NativePermissionsValue => "В самом агенте",
        Text::CodexSandboxTitle => "Ограничивать доступ через sandbox",
        Text::CodexSandboxHint => {
            "При включении — workspace-write, при выключении — полный доступ к файловой системе. Для новых, возобновляемых и повторно запускаемых процессов Codex; уже запущенные процессы не меняются."
        }
        Text::CodexCommandApprovalsTitle => "Запрашивать подтверждение команд",
        Text::CodexCommandApprovalsHint => {
            "При включении — on-request, при выключении — never. Отключение не обходит ограничения sandbox. Эти настройки важнее параметров разрешений в Arguments для новых, возобновляемых и повторно запускаемых процессов."
        }
        Text::OpenCodePermissionsHint => {
            "Задайте разрешения в конфигурации OpenCode или явно в аргументах запуска. Guild их не меняет."
        }
        Text::PermissionsHint => "Для новых сессий Claude Code.",
        Text::LanguageTitle => "Язык",
        Text::LanguageHint => "Применяется ко всему интерфейсу, включая строку меню.",
        Text::Yes => "да",
        Text::No => "нет",
        Text::MenuFile => "Файл",
        Text::MenuEdit => "Правка",
        Text::MenuView => "Вид",
        Text::MenuGo => "Навигация",
        Text::MenuHelp => "Справка",
        Text::MenuSettings => "Настройки…",
        Text::MenuServices => "Услуги",
        Text::MenuHide => "Скрыть Guild",
        Text::MenuHideOthers => "Скрыть остальные",
        Text::MenuShowAll => "Показать все",
        Text::MenuQuit => "Выйти из Guild",
        Text::MenuOpen => "Открыть",
        Text::MenuShowDiff => "Показать различия",
        Text::MenuExpand => "Развернуть",
        Text::MenuCollapse => "Свернуть",
        Text::MenuCopyPath => "Копировать путь",
        Text::MenuCopyFullPath => "Копировать полный путь",
        Text::MenuReveal => "Показать в Finder",
        Text::MenuCloseOthers => "Закрыть остальные",
        Text::MenuCloseAll => "Закрыть все",
        Text::TabsHidden => "Показать скрытые вкладки",
        Text::TabActions => "Недавние файлы, действия с вкладками и прочее",
        Text::NewTabKinds => "Новая вкладка…",
        Text::MenuRecentFiles => "Недавние файлы",
        Text::MenuCloseAllTabs => "Закрыть все вкладки",
        Text::MenuCloseOtherTabs => "Закрыть другие вкладки",
        Text::MenuCloseLeft => "Закрыть вкладки слева",
        Text::MenuCloseRight => "Закрыть вкладки справа",
        Text::MenuCopyFileName => "Копировать имя файла",
        Text::MenuCopyPathWithLine => "Копировать путь со строкой",
        Text::BranchesTab => "Ветки",
        Text::StashesTab => "Стэши",
        Text::BranchSearch => "Переключиться или ввести имя новой ветки…",
        Text::BranchHint => "enter выбирает",
        Text::BranchesLocal => "Локальные ветки",
        Text::BranchesNone => "Ветки с таким именем нет",
        Text::StashesNone => "В стэше пусто",
        Text::MenuDiffStaged => "Показать, что в индексе",
        Text::MenuDiffUnstaged => "Показать, что не в индексе",
        Text::MenuInclude => "Включить в коммит",
        Text::MenuExclude => "Не включать в коммит",
        Text::MenuShowBranchLog => "Показать эту ветку",
        Text::MenuShowHeadLog => "Показать текущую ветку",
        Text::MenuCopyBranchName => "Копировать имя ветки",
        Text::MenuCopyRevisionShort => "Копировать хеш",
        Text::MenuCopyRevision => "Копировать полный хеш",
        Text::MenuCopySubject => "Копировать сообщение",
        Text::MenuRenameTab => "Переименовать вкладку…",
        Text::MenuCloseTab => "Закрыть вкладку",
        Text::MenuReopenTab => "Вернуть закрытую вкладку",
        Text::MenuCopy => "Скопировать",
        Text::MenuPaste => "Вставить",
        Text::MenuLarger => "Крупнее",
        Text::MenuSmaller => "Мельче",
        Text::MenuActualSize => "Исходный размер",
        Text::MenuSessionList => "Список сессий",
        Text::MenuGoToWaitingSession => "К ожидающей сессии",
        Text::MenuToggleColumn => "Рабочие пространства",
        Text::MenuRenameProject => "Переименовать проект…",
        Text::MenuMoveToWorkspace => "Переместить в workspace",
        Text::MenuMoveToGroup => "Переместить в группу",
        Text::WorktreeTabsExpand => "Показать вкладки",
        Text::WorktreeTabsCollapse => "Скрыть вкладки",
        Text::MenuNewProjectGroup => "Новая группа…",
        Text::MenuRemoveFromProjectGroup => "Убрать из группы",
        Text::MenuRenameProjectGroup => "Переименовать группу…",
        Text::MenuDeleteProjectGroup => "Удалить группу",
        Text::ProjectGroupNew => "Новая группа",
        Text::ProjectGroupRename => "Переименовать группу",
        Text::ProjectGroupNamePlaceholder => "Название группы",
        Text::ProjectGroupMerge => "Объединить проекты в группу?",
        Text::ProjectGroupMergeAnd => "и",
        Text::ProjectGroupMergeConfirm => "Объединить",
        Text::MenuGroupNewSession => "Новая сессия",
        Text::MenuGroupNewSessionAsk => "Новая сессия…",
        Text::MenuGroupBindFolder => "Привязать к папке…",
        Text::MenuGroupChangeFolder => "Сменить папку…",
        Text::GroupFolderLine => "Её сессии будут запускаться в этой папке.",
        Text::GroupFolderPlaceholder => "Путь к папке",
        Text::GroupFolderNoSuggestion => "Проекты лежат в разных папках.",
        Text::GroupFolderChoose => "Выбрать папку…",
        Text::GroupFolderBind => "Привязать",
        Text::GroupFolderOwnRoot => "Эта папка — проект одной из групп",
        Text::GroupFolderOtherGroup => "К этой папке уже привязана другая группа",
        Text::GroupFolderNewSession => "Новая сессия в папке группы",
        Text::GroupAddProjects => "Добавить проекты в группу",
        Text::GroupMemberOutside => "вне папки",
        Text::GroupNothingToAdd => "Все проекты этого workspace уже в группе",
        Text::GroupFolderLinked => {
            "Эта папка — связанный worktree или подмодуль; выберите папку, в которой лежат проекты"
        }
        Text::GroupFolderGitBusy => {
            "Git ещё выполняет запись для команды; привяжите папку чуть позже"
        }
        Text::MenuRemoveProject => "Убрать проект",
        Text::MenuRemoveWorktree => "Удалить worktree…",
        Text::ActionCopyWorktreePath => "Скопировать путь worktree",
        Text::MenuUnlockWorktree => "Разблокировать worktree",
        Text::ConfirmRemoveWorktree => "Удалить",
        Text::NoticeWorktreeBusy => "В этом worktree работает агент — сначала останови его",
        Text::NoticeWorktreeChanged => "В этом worktree есть изменения, git их не выбросит",
        Text::ColumnNewWorktree => "Новый worktree…",
        Text::AgentClaudeCode => "Claude Code",
        Text::AgentOpenCode => "OpenCode",
        Text::AgentCodex => "Codex",
        Text::OpenCodeNeedsV2 => "Требуется OpenCode v2 — обновите OpenCode.",
        Text::ResumeStillClosing => {
            "Прежний процесс этого разговора ещё завершается — попробуйте чуть позже."
        }
        Text::SessionsClear => "Всё спокойно",
        Text::SessionStateWaiting => "Ждёт ответа",
        Text::SessionStateFailed => "Ошибка",
        Text::SessionStateWorking => "Работает",
        Text::SessionStateDone => "Готово",
        Text::SessionStateIdle => "Простаивает",
        Text::SessionsWaitingForYou => "Ждут вас",
        Text::SessionsWorking => "Работают",
        Text::SessionsDone => "Готово",
        Text::SessionsIdle => "Простаивают",
        Text::SessionsSettled => "Затихшие",
        Text::SessionFilter => "Фильтр сессий",
        Text::SessionFilterAgent => "Агент",
        Text::SessionFilterState => "Состояние",
        Text::SessionFilterProject => "Проект",
        Text::SessionShowInProjects => "Показать в проектах",
        Text::SessionContinue => "Продолжить",
        Text::SessionsKeys => "↑ ↓ Выбор   Enter Открыть   Esc Закрыть",
        Text::AgentsNone => "Нет открытых агентов",
        Text::AgentsTooltip => "Агенты, открытые в окне",
        Text::NewWorktreePlaceholder => "Ветка нового worktree",
        Text::ColumnOpen => "Открыть…",
        Text::WorktreeDeleteTooltip => "Удалить рабочее дерево",
        Text::WorktreeDeleteTitle => "Удалить это рабочее дерево?",
        Text::WorktreeDeleteDescription => "Папка и игнорируемые файлы будут удалены.",
        Text::WorktreeDeleteBranch => "Также удалить ветку",
        Text::WorktreeDeleteBranchHint => {
            "Удалится только подтверждённая ветка, если Git безопасно разрешит это."
        }
        Text::WorktreeBranchProtected => {
            "Main/master, отделённый HEAD и недоступные ветки сохраняются."
        }
        Text::WorktreeBranchUnavailable => "Ветка недоступна",
        Text::WorktreeDetached => "Отделённый HEAD",
        Text::WorktreeDelete => "Удалить",
        Text::WorktreeRemovalUnavailable => {
            "Репозиторий недоступен. Повторите после загрузки его состояния."
        }
        Text::SidebarPinned => "Закреплённые",
        Text::WorktreePin => "Закрепить рабочее дерево",
        Text::WorktreeUnpin => "Открепить рабочее дерево",
        Text::SidebarProjects => "Проекты",
        Text::SidebarSessions => "Сессии",
        Text::ColumnShowSessions => "Показать сессии",
        Text::ColumnShowProjects => "Показать проекты",
        Text::SidebarProjectsAndGroups => "Проекты и группы",
        Text::ProjectSort => "Сортировка проектов",
        Text::ProjectSortRecent => "Недавняя активность",
        Text::ProjectSortMost => "Наибольшая активность",
        Text::ProjectSortUser => "Последнее сообщение пользователя",
        Text::ProjectSortManual => "Вручную",
        Text::NoticeNobodyWaiting => "Ни одна сессия не ждёт ответа",
        Text::NoticeWorktreeGone => "Папки этого worktree больше нет",
        Text::MenuFullScreen => "Полный экран",
        Text::TerminalPreviousCommand => "Предыдущая команда",
        Text::TerminalNextCommand => "Следующая команда",
        Text::TerminalSelectOutput => "Выделить вывод команды",
        Text::TerminalCopyOutput => "Копировать вывод команды",
        Text::TerminalExitStatus => "Код завершения",
        Text::MenuClearTerminal => "Очистить терминал",
        Text::MenuNextTab => "Следующая вкладка",
        Text::MenuPreviousTab => "Предыдущая вкладка",
        Text::MenuNextProject => "Следующий проект",
        Text::MenuNextWorkspace => "Следующее рабочее пространство",
        Text::MenuPreviousWorkspace => "Предыдущее рабочее пространство",
        Text::MenuSplitAreaRight => "Разделить вправо",
        Text::MenuSplitAreaDown => "Разделить вниз",
        Text::MenuSplitPaneRight => "Разделить вкладку вправо",
        Text::MenuSplitPaneDown => "Разделить вкладку вниз",
        Text::MenuCloseArea => "Закрыть область",
        Text::ActionFocusLeft => "Фокус влево",
        Text::ActionFocusRight => "Фокус вправо",
        Text::ActionFocusUp => "Фокус вверх",
        Text::ActionFocusDown => "Фокус вниз",
        Text::LastAreaStays => "последняя область остаётся",
        Text::NoticeLastAreaStays => "Последняя область остаётся",
        Text::NoticeNoRoomToSplit => "Для разделения этой области не хватает места",
        Text::NoticeNoRoomToSplitTab => "Для разделения этой вкладки не хватает места",
        Text::MenuMoveToArea => "Переместить в область",
        Text::MenuAreaLeft => "Левая область",
        Text::MenuAreaRight => "Правая область",
        Text::MenuAreaAbove => "Область выше",
        Text::MenuAreaBelow => "Область ниже",
        Text::MenuNewAreaRight => "Новая область справа",
        Text::MenuNewAreaBelow => "Новая область снизу",
        Text::WorkspaceNew => "Новое рабочее пространство",
        Text::WorkspaceNamePlaceholder => "Название рабочего пространства",
        Text::WorkspaceRenameAction => "Переименовать",
        Text::WorkspaceRename => "Переименовать рабочее пространство",
        Text::WorkspaceChangeIcon => "Изменить значок",
        Text::WorkspaceChooseIcon => "Выбрать значок",
        Text::WorkspaceDelete => "Удалить",
        Text::WorkspaceNoIcon => "Без значка",
        Text::WorkspaceCapacity => "Можно создать не больше пяти рабочих пространств.",
        Text::WorkspaceLastCannotDelete => "Последнее рабочее пространство нельзя удалить.",
        Text::WorkspaceOnlyOne => "Рабочее пространство только одно.",
        Text::MenuRecentProjects => "Недавние проекты…",
        Text::MenuShowLog => "Показать журнал",
        Text::MenuNew => "Создать",
        Text::MenuExistingSources => "Проект из существующих исходников…",
        Text::MenuNewProject => "Проект…",
        Text::MenuOpenProject => "Открыть…",
        Text::MenuCloseProject => "Закрыть проект",
        Text::MenuAbout => "О Guild",
        Text::MenuUpdateGuild => "Обновить Guild…",
        Text::UpdateSourceSearch => "Поиск ветки-источника…",
        Text::UpdateTerminal => "Открыть терминал Guild",
        Text::UpdateTerminalUnavailable => {
            "Откройте исходный worktree в этом workspace для доступа к его терминалу."
        }
        Text::UpdateSourceLocal => "только локальная",
        Text::UpdateBranch => "Обновить ветку…",
        Text::UpdateBranchConfirm => "Обновить",
        Text::UpdateBranchRunning => "Обновление ветки…",
        Text::UpdateBranchChoose => "Выбрать источник…",
        Text::UpdateBranchUnsaved => "Сохраните открытые файлы перед обновлением ветки.",
        Text::UpdaterReplaceTitle => "Заменить установленную сборку?",
        Text::UpdaterReplaceAction => "Заменить и перезапустить",
        Text::UpdaterUpToDate => "Эта сборка уже установлена.",
        Text::UpdateButton => "Обновить",
        Text::WhatsNewTitle => "Что нового в Guild",
        Text::WhatsNewContinue => "Продолжить",
        Text::UsageSession => "Сессия",
        Text::UsageWeek => "Неделя",
        Text::UsageAllProviders => "Все провайдеры",
        Text::UsageNotInstalled => "не установлен",
        Text::UsageCouldNotRead => "не удалось прочитать",
        Text::UsageRateLimited => "запросы ограничены",
        Text::UsageNotSignedIn => "вход не выполнен",
        Text::UsageNothingToRead => "лимитов нет",
        Text::UsageRefresh => "Обновить",
        Text::UsageAsking => "запрашиваю…",
        Text::OpenSelectedApp => "Открыть",
        Text::OpenInAppsTitle => "Приложения «Открыть в»",
        Text::OpenInAppsHint => "Выберите приложения, их видимость и порядок в списке.",
        Text::OpenInAdd => "Добавить приложение",
        Text::OpenInName => "Отображаемое имя",
        Text::OpenInChoose => "Выбрать приложение…",
        Text::OpenInSearch => "Поиск приложений…",
        Text::OpenInRemove => "Удалить",
        Text::OpenInUnavailable => "Не установлено или не подключено",
        Text::OpenInLoading => "Загрузка приложений…",
        Text::OpenInNoResults => "Приложения не найдены",
        Text::OpenInMoveUp => "Переместить выше",
        Text::OpenInMoveDown => "Переместить ниже",
        Text::OpenInButton => "Открыть в…",
        Text::NoticeRestartingToUpdate => "Перезапуск, чтобы применить обновление...",
        Text::NoticeUpdateNothingStaged => {
            "Нового релиза нет. just release на main в checkout Guild подготовит его, а кнопка «Обновить» применит."
        }
        Text::NoticeUpdateNotABundle => "Этот Guild запущен не из бандла — заменять нечего.",
        Text::NoticeUpdateNoHelper => {
            "В бандле этого Guild нет swap-guild.sh — установите его один раз через just install."
        }
        Text::ConfirmClose => "Закрыть",
        Text::MenuOpenRecent => "Недавние",
        Text::MenuClearRecent => "Очистить список",
        Text::MenuNoRecent => "Пока пусто",
        Text::NoticeWorktreeOrderUnsupportedPath => {
            "Порядок worktree не изменён: путь одного из них содержит недопустимые для UTF-8 байты"
        }
        Text::NoticeStateNotSaved => "Состояние не сохранено",
        Text::NoticeSettingsNotSaved => "Настройки не сохранены",
        Text::NoticeKeymapNotSaved => "Keymap не сохранён",
        Text::NoticeKeymapNotRead => "Файл keymap не прочитан — действуют умолчания",
        Text::ThemeTitle => "Тема",
        Text::ThemeHint => {
            "Системная следует оформлению macOS; терминал и редакторы перекрашиваются вместе с окном"
        }
        Text::ThemeSystem => "Системная",
        Text::ThemeLight => "Светлая",
        Text::ThemeDark => "Тёмная",
        Text::CustomizeThemeTitle => "Настроить тему",
        Text::CustomizeThemeHint => {
            "Оттенок фона, акцента и выделения светлой или тёмной темы. Изменения видны сразу; «Сохранить» их оставляет"
        }
        Text::CustomizeThemeClose => "Закрыть без сохранения",
        Text::ThemeBackground => "Фон",
        Text::ThemeAccent => "Акцент",
        Text::ThemeHighlight => "Выделение",
        Text::ThemeHue => "Тон",
        Text::ThemeSaturation => "Насыщенность",
        Text::ThemeIntensity => "Интенсивность",
        Text::ThemeCopyFromLight => "Взять из светлой",
        Text::ThemeCopyFromDark => "Взять из тёмной",
        Text::ThemeSaveLight => "Сохранить светлую тему",
        Text::ThemeSaveDark => "Сохранить тёмную тему",
        Text::ThemeReset => "Сбросить",
        Text::ThemeEyedropper => "Взять цвет с экрана",
        Text::ThemeHexInvalid => "Это не цвет — введите #rrggbb или #rgb",
        Text::ThemeDefaultSwatch => "По умолчанию",
        Text::AppearanceSection => "Внешний вид",
        Text::UiFontSizeTitle => "Размер шрифта интерфейса",
        Text::UiFontSizeHint => "Всё, кроме терминала: вкладки, строка проектов, меню и этот экран",
        Text::UiFontTitle => "Шрифт интерфейса",
        Text::TerminalFontTitle => "Шрифт терминала",
        Text::TerminalFontHint => "Шрифт вкладок терминала.",
        Text::CodeFontTitle => "Моноширинный шрифт",
        Text::CodeFontHint => "Шрифт кода и диффов.",
        Text::FontCustom => "Другой",
        Text::FontBeta => "бета",
        Text::FontFromSystem => "Системный",
        Text::LogDirectoryTitle => "Журнал",
        Text::PanelProject => "Проводник",
        Text::PanelSessions => "Сессии",
        Text::PanelTerminal => "Терминал",
        Text::NewPanelTerminal => "Новый терминал",
        Text::ClosePanelTerminal => "Закрыть терминал и завершить его процессы",
        Text::PanelTerminalEmpty => "Открыть терминал в этом проекте",
        Text::PanelOutput => "Запуск",
        Text::RecentSearch => "Поиск среди недавних проектов",
        Text::RecentNoMatch => "Ни один недавний проект не подходит",
        Text::RecentNever => "Ещё ничего не открывали — «Файл ▸ Открыть»",
        Text::PanelProjectEmpty => "Здесь будут файлы проекта",
        Text::TreeEmpty => "Пока пусто",
        Text::TreeCannotRead => "Этот каталог не удалось прочитать",
        Text::ImageSource => "Исходник",
        Text::ImageFit => "Вписать",
        Text::ImageLoading => "Загрузка изображения…",
        Text::ImageCannotRead => "Не удалось открыть изображение",
        Text::FileNotText => "Это не текстовый файл",
        Text::FileTooLarge => "Слишком большой, чтобы открыть здесь",
        Text::FileGone => "Файла больше нет на диске",
        Text::FileCannotRead => "Файл не удалось прочитать",
        Text::SettingsMarkdown => "Файлы Markdown",
        Text::MarkdownDefaultViewTitle => "Режим Markdown по умолчанию",
        Text::MarkdownDefaultViewHint => {
            "Для вновь открываемых файлов Markdown во всех проектах. Открытые панели сохраняют свой режим."
        }
        Text::MarkdownEditor => "Редактор",
        Text::MarkdownSplit => "Редактор и просмотр",
        Text::MarkdownPreview => "Просмотр",
        Text::DiagramRendering => "Построение диаграммы…",
        Text::DiagramResourcesUnavailable => {
            "Ресурсы диаграмм отсутствуют или повреждены. Переустановите Guild; исходный текст можно редактировать."
        }
        Text::MarkdownPreviewLimit => {
            "Предпросмотр недоступен для большого документа. Продолжайте в редакторе."
        }
        Text::FileChangedOnDisk => "Изменён на диске, пока правился здесь",
        Text::MenuSave => "Сохранить",
        Text::MenuSaveAll => "Сохранить всё",
        Text::MenuReloadFile => "Перечитать с диска",
        Text::UnsavedDetail => "Без сохранения они пропадут.",
        Text::Save => "Сохранить",
        Text::DontSave => "Не сохранять",
        Text::RevealActiveFile => "Показать открытый файл",
        Text::NoFileInFront => "в этой вкладке нет файла",
        Text::ShowDiff => "Показать диф",
        Text::ShowChanges => "Показать изменения",
        Text::DiffChanges => "Диф",
        Text::DiffLoading => "Читаю последний коммит\u{2026}",
        Text::DiffNoChanges => "Изменений с последнего коммита нет",
        Text::DiffBinary => "Двоичный файл, дифа нет",
        Text::DiffTooLarge => "Слишком большой для дифа здесь",
        Text::DiffNoRepository => "Не в git-репозитории",
        Text::DiffNoGit => "git на этой машине не установлен",
        Text::Changes => "Изменения",
        Text::Log => "Лог",
        Text::PanelChangesEmpty => "Изменений с последнего коммита нет",
        Text::Unversioned => "Неверсионированные файлы",
        Text::NextChange => "Следующее изменение",
        Text::RollbackLines => "Откатить изменение",
        Text::PreviousChange => "Предыдущее изменение",
        Text::BaseStaged => "В индексе",
        Text::BaseCurrent => "Текущая версия",
        Text::BaseIndex => "в индексе",
        Text::BaseUnstaged => "не в индексе",
        Text::Refresh => "Обновить",
        Text::ShowSelectedDiff => "Показать диф",
        Text::TickAll => "Отметить все",
        Text::UntickAll => "Снять все",
        Text::CollapseAll => "Свернуть все",
        Text::ExpandAll => "Развернуть все",
        Text::CommitPlaceholder => "Сообщение коммита",
        Text::CommitButton => "Закоммитить",
        Text::CommitNothingStaged => "Ничего не отмечено",
        Text::CommitNoMessage => "Сначала напишите сообщение",
        Text::MenuCommit => "Коммит…",
        Text::LogLoading => "Читаю\u{2026}",
        Text::LogNoMatches => "Под фильтр не подходит ни один коммит",
        Text::LogLocal => "Локальные",
        Text::LogSelectCommit => "Выберите коммит, чтобы увидеть его",
        Text::DiffViewTitle => "Вид дифа",
        Text::DiffViewHint => {
            "Две колонки, где изменённые строки друг напротив друга, или одна, как печатает git"
        }
        Text::DiffSideBySide => "в две колонки",
        Text::DiffUnified => "в одну колонку",
        Text::ToolCommit => "Коммит",
        Text::ToolSearch => "Поиск",
        Text::ToolProblems => "Проблемы",
        Text::ToolFind => "Использования",
        Text::PanelFindEmpty => "Нажмите ⌥F7 на имени, чтобы найти его использования",
        Text::UsagesInProjectFiles => "Использования в файлах проекта",
        Text::UsagesInLibraries => "Использования в библиотеках",
        Text::UsagesSearching => "Поиск…",
        Text::UsagesCannotSearch => "Отсюда нельзя искать использования.",
        Text::UsagesCannotSearchHint => {
            "Поставьте курсор на элемент, использования которого нужно найти, и повторите."
        }
        Text::UsagesNoPreview => "Для файла библиотеки превью нет",
        Text::FindRerun => "Повторить",
        Text::FindExpandAll => "Развернуть всё",
        Text::FindCollapseAll => "Свернуть всё",
        Text::ActionFindUsages => "Найти использования",
        Text::ActionNextUsage => "Следующее вхождение",
        Text::ActionPreviousUsage => "Предыдущее вхождение",
        Text::ToolGit => "Git",
        Text::ConsoleTab => "Консоль",
        Text::LogSearch => "Текст или хеш",
        Text::LogBranchFilter => "Ветка",
        Text::LogUserFilter => "Автор",
        Text::LogDateFilter => "Дата",
        Text::LogAnyone => "Любой",
        Text::LogAnyTime => "За всё время",
        Text::LogLastDay => "За сутки",
        Text::LogLastWeek => "За неделю",
        Text::LogLastMonth => "За месяц",
        Text::LogHead => "HEAD",
        Text::LogAllBranches => "Все ветки",
        Text::LogAllBranchesRow => "Все ветки",
        Text::LogHeadRow => "HEAD (текущая ветка)",
        Text::LogBranchSearch => "Ветка или тег",
        Text::LogLocalBranches => "Локальные",
        Text::LogRecentBranches => "Недавние",
        Text::LogClearHint => "Нажмите \u{232b}, чтобы снять фильтр",
        Text::ConsoleEmpty => "С этим репозиторием пока ничего не делали",
        Text::MoveTo => "Переместить",
        Text::PanelOptions => "Параметры",
        Text::PanelHide => "Скрыть",
        Text::DockLeft => "Влево",
        Text::DockRight => "Вправо",
        Text::DockBottom => "Вниз",
        Text::DockEmpty => "Здесь пусто",
        Text::DockEmptyHint => {
            "Перетащите сюда инструмент из строки состояния или переместите его из меню «Вид»"
        }
        Text::Annotate => "Аннотировать",
        Text::AnnotateTab => "авторы",
        Text::AnnotateNotCommitted => "Ещё не в коммите",
        Text::PanelLogEmpty => "Коммитов ещё нет",
        Text::PanelSessionsEmpty => "Здесь будут сессии и задачи проекта",
        Text::PanelOutputEmpty => "Здесь будут сборки, тесты и логи",
        Text::PanelSearchEmpty => "Введите текст, чтобы искать в проекте",
        Text::PanelProblemsEmpty => "Здесь всё в порядке",
        Text::SearchPlaceholder => "Найти в файлах…",
        Text::SearchMask => "Маска файлов",
        Text::SearchMatchCase => "С учётом регистра",
        Text::SearchWholeWord => "Слова целиком",
        Text::SearchRegex => "Регулярное выражение",
        Text::SearchNothingFound => "Ничего не найдено",
        Text::SearchRunning => "идёт поиск…",
        Text::SearchCapped => "и ещё",
        Text::RunNothingYet => "Здесь ещё ничего не запускали",
        Text::RunStart => "Запустить",
        Text::RunAgain => "Запустить снова",
        Text::RunClosePrompt => "Остановить и закрыть этот запуск?",
        Text::RunStopAndClose => "Остановить и закрыть",
        Text::RunStop => "Остановить",
        Text::RunNotRunning => "Ничего не выполняется",
        Text::RunClear => "Очистить",
        Text::RunChoose => "Выберите, что запустить",
        Text::RunTaskSettings => "Аргументы запуска",
        Text::RunConfigure => "Запуск с аргументами: Option-клик",
        Text::RunArgumentsRequired => "Обязательный аргумент",
        Text::RunVariables => "Необязательные NAME=value (пробелы в кавычках)",
        Text::RunDefault => "Задача по умолчанию",
        Text::BlameUnavailable => "Аннотации недоступны",
        Text::BlameCommitUnavailable => "Исторический коммит недоступен",
        Text::BlameFileUnavailable => "Исторический файл недоступен в этом коммите",
        Text::RunSourceChanged => "Источник задачи изменился; выберите задачу снова",
        Text::RunFailed => "не запустилась",
        Text::RunPlaceholder => "Что запустить…",
        Text::RunRecent => "недавнее",
        Text::RunDeclared => "объявлено",
        Text::RunNothingToOffer => "Напишите команду, чтобы запустить её",
        Text::MenuRun => "Запуск",
        Text::FontBundled => "в составе Guild",
        Text::FontSearch => "Найти шрифт…",
        Text::FontNoneFound => "Нет такого семейства",
        Text::ServerNone => "без сервера",
        Text::ServerStarting => "сервер запускается…",
        Text::ServerReady => "готов",
        Text::ServersNone => "В этом проекте не запускался ни один языковой сервер",
        Text::ServersTooltip => "Языковые серверы",
        Text::XcodeSetUp => "Настроить для Xcode",
        Text::XcodeSettingUp => "Настраиваю для Xcode…",
        Text::GoToDefinition => "К определению",
        Text::BaseProposal => "предложено",
        Text::AcceptProposal => "Применить",
        Text::AcceptProposalHint => {
            "Агент ждёт: правку сделает он сам, и терминал больше не спросит"
        }
        Text::RejectProposal => "Отклонить",
        Text::RejectProposalHint => "Агент ждёт: ничего не будет записано",
        Text::ProjectsSection => "Проекты",
        Text::ReopenLastTitle => "Открывать последний проект при запуске",
        Text::ReopenLastHint => "Вместе с его вкладками",
        Text::AutoFastForwardTitle => "Автоматически перематывать ветку по умолчанию",
        Text::AutoFastForwardHint => {
            "После каждого фонового fetch — при открытии проекта и каждые 5 минут — чистая ветка по умолчанию догоняет свой upstream; разошедшаяся ветка не трогается"
        }
        Text::GitLabMrTitle => "Merge request'ы GitLab в колонке",
        Text::GitLabMrHint => {
            "Показывать merge request каждого worktree и что мешает его слить — по вашему входу в glab. Выключено — Guild не начинает новых запросов к GitLab; уже запущенный доработает, а его ответ отбрасывается."
        }
        Text::MenuViewMr => "Открыть MR",
        Text::MenuCopyMrUrl => "Скопировать ссылку на MR",
        Text::ConfirmCloseTitle => "Спрашивать, прежде чем закрыть работающего агента",
        Text::ConfirmCloseHint => {
            "Вкладка, проект или его удаление — когда агент работает или ждёт вас"
        }
        Text::ConfirmCloseAgentTitle => "Спрашивать перед закрытием вкладки с агентом",
        Text::ConfirmCloseAgentHint => {
            "Claude Code, Codex или opencode — даже когда агент ничего не делает или закончил"
        }
        Text::ForgetOnCloseTitle => "Забывать сессии проекта при закрытии",
        Text::ForgetOnCloseHint => {
            "Выключено — открытый заново проект возвращается со своими разговорами"
        }

        // ---- M8.8.5 — the status bar and Problems ----
        Text::GoToLine => "Перейти к строке и колонке",
        Text::SoftWrapOn => "Перенос строк включён (нажмите, чтобы выключить)",
        Text::SoftWrapOff => "Перенос строк выключен (нажмите, чтобы включить)",
        Text::LineEndings => "Концы строк",
        Text::ProblemsNoServer => "Здесь ещё не смотрел языковой сервер",
        Text::ProblemsLooking => "Ещё смотрим…",
        Text::ProblemsCapped => "и ещё",
        // Дерево файлов — M8.8.3.
        Text::MenuNewFile => "Файл…",
        Text::MenuNewDirectory => "Каталог…",
        Text::MenuOpenIn => "Открыть в",
        Text::MenuOpenTerminal => "Терминале",
        Text::MenuFindInFiles => "Найти в файлах…",
        Text::MenuReplaceInFiles => "Заменить в файлах…",
        Text::MenuMove => "Переместить…",
        Text::MenuCutFiles => "Вырезать",
        Text::MenuCopyFiles => "Копировать",
        Text::MenuPasteFiles => "Вставить",
        Text::FileTransferCompleted => "Выполнено",
        Text::FileTransferRemaining => "Не перенесено",
        Text::FileClipboardChanged => {
            "Файлы в буфере обмена изменились. Выберите и скопируйте или вырежьте их заново."
        }
        Text::FileTransferBusy => "Перенос файлов…",
        Text::TrashUnsaved => {
            "Несохранённые изменения останутся открытыми. Последующее сохранение может восстановить удалённые файлы."
        }
        Text::FileTransferDestination => "Каталог относительно проекта (пусто = корень)",
        Text::FileTransferCollision => "Эти имена уже заняты",
        Text::FileTransferSkip => "Пропустить",
        Text::FileTransferKeepBoth => "Сохранить оба",
        Text::MenuRename => "Переименовать…",
        Text::MenuDelete => "Удалить",
        Text::MenuAnnotate => "Аннотировать",
        Text::MenuAnnotateWithBlame => "Аннотировать с помощью Git Blame",
        Text::MenuAddToVcs => "Добавить в VCS",
        Text::TreeNewFile => "Новый файл",
        Text::TreeNewDirectory => "Новый каталог",
        Text::TreeRename => "Переименовать",
        Text::Delete => "В Корзину",
        Text::SearchScope => "Где",
        Text::ExternalLibraries => "Внешние библиотеки",
        // Навигация
        Text::MenuBack => "Назад",
        Text::MenuForward => "Вперёд",
        Text::MenuLastEdit => "К последней правке",
        Text::MenuRecentLocations => "Недавние места",
        Text::MenuGoToLine => "Перейти к строке:столбцу…",
        Text::MenuSelectInExplorer => "Показать в дереве",
        Text::RecentFilesSearch => "Поиск среди недавних файлов",
        Text::RecentFilesNever => "Ни одного файла этого проекта ещё не открывали",
        Text::RecentFilesNoMatch => "Ни один недавний файл не подходит",
        Text::RecentFilesNoEdits => "Здесь пока ничего не изменяли",
        Text::RecentFilesEdited => "Только изменённые",
        Text::RecentFilesCloseHint => "⌫ закрывает",
        Text::RecentLocationsSearch => "Поиск среди недавних мест",
        Text::RecentLocationsNever => "Пока никуда не переходили",
        Text::GoToLinePlaceholder => "строка:столбец",
        // git: меню панели коммита и Push — M8.8.4.
        Text::MenuRollback => "Откатить…",
        Text::Rollback => "Откатить",
        Text::PushTab => "Отправка",
        Text::PushDialogTitle => "Отправка коммитов",
        Text::PushTagsLabel => "Отправить теги:",
        Text::PushTagsAll => "Все",
        Text::PushTagsCurrent => "Текущей ветки",
        Text::PushHooks => "Запускать Git hooks",
        Text::PushDetails => "Показать детали",
        Text::PushPreview => "Предпросмотр diff",
        Text::PushTarget => "Целевая ветка",
        Text::PushTargetApply => "Применить",
        Text::PushEmpty => "Коммиты не выбраны",
        Text::PushHelp => {
            "Выберите ветку для просмотра всех изменений или коммит для просмотра его файлов и деталей. Command-клик выбирает несколько коммитов, Shift-клик — диапазон. Отправляется всегда вся ветка. Git hooks запускает pre-push hook репозитория."
        }
        Text::PushNew => "Новая",
        Text::DiffEditSource => "Открыть в редакторе",
        Text::DiffWindowNextFile => "Следующий файл",
        Text::DiffWindowPreviousFile => "Предыдущий файл",
        Text::PushButton => "Отправить",
        Text::PushRunning => "Отправляем…",
        Text::PushSetUpstream => "Задать upstream",
        Text::PushNoRemote => "Не задан удалённый репозиторий",
        Text::PushDetached => "HEAD отделён: отправлять нечего",
        Text::ChangesSelectRow => "Сначала выберите изменение",
        Text::RollbackNothingToUndo => "git не знает этот файл: возвращать нечего",
        Text::AlreadyInVcs => "git уже знает этот файл",
        // Острые края аудита — M5.11.
        Text::FileReadOnly => "это исходник зависимости: здесь он читается, но не записывается",
        Text::KeymapNeedsModifier => "добавьте cmd, ctrl или alt",
        Text::DropStash => "Удалить",
        Text::NoticeStashGone => "Этой записи в stash больше нет — ничего не удалено",
        Text::NoticeHunkStale => {
            "Файл изменился после показа изменений — они пересчитаны, повторите"
        }
        Text::NoticeHunkConverted => {
            "Git преобразует этот файл при сохранении — добавьте файл целиком"
        }
        Text::NoticeHunkHeadMoved => {
            "Пока изменение снималось, появился коммит — проверьте Staged diff"
        }
        Text::NoticeHunkUnverified => {
            "Git записал изменение, но индекс не удалось перечитать — проверьте Staged diff"
        }
        Text::NoticeHunkUnconfirmed => {
            "Guild не смог проверить, записал ли Git изменение — проверьте Staged diff"
        }
        Text::StageHunk => "Добавить изменение в индекс",
        Text::UnstageHunk => "Убрать изменение из индекса",
        Text::HunkWider => "В индексе изменение шире этого — используйте Staged или Unstaged diff",
        Text::HunkMixed => {
            "Часть этого изменения уже в индексе — добавьте или уберите его в Unstaged или Staged diff"
        }
        Text::HunkWholeFile => "Этот файл добавляется в индекс только целиком",
        Text::HunkLineEndings => {
            "Это изменение отличается от индекса только концами строк — добавьте файл целиком"
        }
        Text::NoticeHunkCommitCancelled => {
            "Коммит не сделан: проверьте индекс и нажмите Commit снова"
        }
        Text::NoticeLoginPathFailed => {
            "Оболочка входа не назвала PATH вовремя — сессии и языковые серверы получат тот, \
             с которым запущен Guild"
        }
        // Терминал для агентов — M5.12.
        Text::MenuSelectAll => "Выделить всё",
        Text::MenuFind => "Найти…",
        Text::FindPlaceholder => "Найти в выводе",
        Text::FindOlder => "Следующее совпадение, выше",
        Text::FindNewer => "Предыдущее совпадение, ниже",
        Text::FindClose => "Закрыть поиск",
        Text::FindScreenOnly => "Эта программа рисует только на экране: ищется лишь то, что на нём",
        // Поиск и навигация — M8.9.
        Text::MenuSearchEverywhere => "Искать везде",
        Text::MenuGoToFile => "Перейти к файлу…",
        Text::MenuGoToSymbol => "Перейти к символу…",
        Text::MenuFindAction => "Найти действие…",
        Text::EverywhereAll => "Все",
        Text::EverywhereFiles => "Файлы",
        Text::EverywhereSymbols => "Символы",
        Text::EverywhereActions => "Действия",
        Text::EverywhereText => "Текст",
        Text::EverywhereNonProject => "Включая элементы вне проекта",
        Text::EverywherePreview => "Предпросмотр",
        Text::EverywhereIndexing => "Индексация…",
        Text::EverywhereTypeToSearch => "Введите запрос",
        Text::ReplaceInFilesTitle => "Заменить в файлах",
        Text::FindInFilesTitle => "Найти в файлах",
        Text::FindInFilesPin => "Не закрывать окно",
        Text::FindInFilesUnpin => "Закрывать окно после перехода",
        Text::FindInFilesOpenInWindow => "Открыть в окне поиска",
        Text::FindInFilesInProject => "В проекте",
        Text::FindInFilesDirectory => "Каталог",
        Text::FindInFilesChooseDirectory => "Выбрать каталог…",
        Text::FindInFilesUseMask => "Учитывать маску файлов",
        Text::FindInFilesHistory => "Недавние запросы",
        Text::FindInFilesMaskHistory => "Недавние маски",
        Text::FindInFilesClose => "Закрыть",
        Text::ActionNewSession => "Новая сессия",
        Text::ActionNewTerminal => "Новый терминал",
        Text::ActionCloseOtherTabs => "Закрыть другие вкладки",
        Text::ActionCloseAllTabs => "Закрыть все вкладки",
        Text::ActionSaveFile => "Сохранить файл",
        Text::ActionSaveAllFiles => "Сохранить все файлы",
        Text::ActionRun => "Запустить команду",
        Text::ActionStopRun => "Остановить команду",
        Text::ActionClearRun => "Очистить вывод",
        Text::ActionAddProject => "Открыть проект…",
        Text::ActionNewProject => "Новый проект…",
        Text::ActionIncreaseFontSize => "Увеличить шрифт терминала",
        Text::ActionDecreaseFontSize => "Уменьшить шрифт терминала",
        Text::ActionResetFontSize => "Сбросить шрифт терминала",
        Text::ActionToggleSidebar => "Показать или скрыть рабочие пространства",
        Text::ActionGoToDefinition => "Перейти к определению",
        Text::ActionFind => "Найти…",
        Text::ActionReplace => "Заменить…",
        Text::ActionNextChange => "Следующее изменение",
        Text::ActionPreviousChange => "Предыдущее изменение",
        Text::ActionStageHunk => "Добавить изменение в индекс",
        Text::ActionUnstageHunk => "Убрать изменение из индекса",
        Text::ActionRefreshChanges => "Обновить изменения",
        Text::ActionShowSelectedDiff => "Показать диф выбранного изменения",
        Text::ActionIncludeSelectedChanges => "Включить выбранные изменения в коммит",
        Text::ActionExcludeSelectedChanges => "Исключить выбранные изменения из коммита",
        Text::ActionTickAllChanges => "Отметить все изменения",
        Text::ActionUntickAllChanges => "Снять отметки со всех изменений",
        Text::ActionCollapseChanges => "Свернуть все изменения",
        Text::ActionExpandChanges => "Развернуть все изменения",
        Text::ActionPreviousProject => "Предыдущий проект",
        Text::ActionShowTabList => "Показать список вкладок",
        Text::ActionPush => "Отправить коммиты…",
        Text::ActionNewFile => "Новый файл…",
        Text::ActionNewDirectory => "Новый каталог…",
        Text::ActionRenameFile => "Переименовать файл…",
        Text::ActionJumpToSource => "Перейти к исходнику",
        Text::ActionDeleteFile => "Переместить файл в корзину…",
        Text::ActionToggleLeftPanel => "Показать или скрыть левую панель",
        Text::ActionToggleRightPanel => "Показать или скрыть правую панель",
        Text::ActionToggleBottomPanel => "Показать или скрыть нижнюю панель",
        Text::ActionWidenPanel => "Расширить панель",
        Text::ActionNarrowPanel => "Сузить панель",
        Text::ActionStretchToLeft => "Растянуть влево",
        Text::ActionStretchToRight => "Растянуть вправо",
        Text::ActionStretchToTop => "Растянуть вверх",
        Text::ActionStretchToBottom => "Растянуть вниз",
        Text::ActionToggleSessions => "Показать или скрыть список сессий",
        Text::ActionToggleFullScreen => "Переключить полноэкранный режим",
        Text::ActionShowLog => "Показать журнал Guild",
    }
}
