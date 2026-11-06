//! Console translations: (Uzbek source, English, Russian).
//!
//! The Uzbek text is the key: it must match the literal passed to `tr` / `tf!` exactly.
//! Placeholders such as `{n}` must appear unchanged in every column.

use vimora_ui::i18n::Row;

pub static ROWS: &[Row] = &[
    // --- window, header, navigation -----------------------------------------------------------
    ("Vimora — O'qituvchi konsoli", "Vimora — Teacher console", "Vimora — Консоль учителя"),
    ("Qulflash", "Lock", "Блокировать"),
    ("unlock|Ochish", "Unlock", "Разблокировать"),
    ("Ochish", "Open", "Открыть"),
    ("Namoyish", "Present", "Показ"),
    ("To'xtatish", "Stop", "Стоп"),
    ("Chat", "Chat", "Чат"),
    ("Yuborish", "Send", "Отправить"),
    ("Cheklash", "Restrict", "Ограничить"),
    ("Tizim", "System", "Система"),
    ("Sozlamalar", "Settings", "Настройки"),
    ("Yorug' rejim", "Light mode", "Светлая тема"),
    ("Qorong'i rejim", "Dark mode", "Тёмная тема"),
    ("Jurnal", "Journal", "Журнал"),
    ("Yangi versiya: {v}", "New version: {v}", "Новая версия: {v}"),
    ("{n} kompyuter", "{n} computers", "{n} компьютеров"),
    ("{n} onlayn", "{n} online", "{n} в сети"),
    ("Sayt ochish", "Open a website", "Открыть сайт"),
    ("Barcha ekranlarda bir sahifa", "One page on every screen", "Одна страница на всех экранах"),
    ("Dastur ishga tushirish", "Start a program", "Запустить программу"),
    ("Masalan, Word yoki Scratch", "For example Word or Scratch", "Например, Word или Scratch"),
    ("Fayl yuborish", "Send a file", "Отправить файл"),
    ("Dars materiali, topshiriq", "Lesson material, assignment", "Материал урока, задание"),
    ("Internet", "Internet", "Интернет"),
    ("Saytlar, kalit so'zlar, SafeSearch", "Websites, keywords, SafeSearch", "Сайты, ключевые слова, SafeSearch"),
    ("Dasturlarni bloklash", "Block applications", "Блокировка программ"),
    ("O'yinlar, messenjerlar", "Games, messengers", "Игры, мессенджеры"),
    ("Shadow rejim", "Shadow mode", "Режим Shadow"),
    ("Qayta yuklashda toza holat", "Clean state on every restart", "Чистое состояние при перезагрузке"),
    ("Yangilanishlar", "Updates", "Обновления"),
    ("Windows, dasturlar, Vimora", "Windows, applications, Vimora", "Windows, программы, Vimora"),
    ("Quvvat", "Power", "Питание"),
    ("O'chirish, qayta yuklash", "Shut down, restart", "Выключение, перезагрузка"),
    // --- room bar, grid, list -------------------------------------------------------------------
    ("Hammasi", "All", "Все"),
    ("Hammasi ({n})", "Everyone ({n})", "Все ({n})"),
    ("Xonasiz", "No room", "Без класса"),
    ("Xona", "Room", "Класс"),
    ("Yangi xona qo'shish", "Add a room", "Добавить класс"),
    ("Yangi xona", "New room", "Новый класс"),
    ("Yangi xona…", "New room…", "Новый класс…"),
    ("Xona nomini o'zgartirish", "Rename room", "Переименовать класс"),
    ("Xonani o'chirish", "Delete room", "Удалить класс"),
    ("\"{name}\" nomli xona allaqachon bor", "A room named \"{name}\" already exists", "Класс «{name}» уже существует"),
    ("Xonaga biriktirish", "Assign to room", "Назначить в класс"),
    ("Xonadan chiqarish", "Remove from room", "Убрать из класса"),
    ("Masalan: 204-xona, Informatika", "For example: Room 204, Computer science", "Например: каб. 204, Информатика"),
    ("Kompyuterlarni o'ng tugma → \"Xonaga biriktirish\" orqali qo'shasiz.", "Add computers with right-click → \"Assign to room\".", "Компьютеры добавляются правым кликом → «Назначить в класс»."),
    ("Kompyuterlar hali ulanmagan", "No computers connected yet", "Компьютеры ещё не подключены"),
    ("O'quvchi kompyuterlariga Vimora'ni o'rnating — ular shu yerda o'zi paydo bo'ladi.\nHech narsa sozlash shart emas: bir xil tarmoqda (Wi-Fi yoki LAN) bo'lsa kifoya.", "Install Vimora on the student computers — they appear here by themselves.\nNothing to configure: being on the same network (Wi-Fi or LAN) is enough.", "Установите Vimora на компьютеры учеников — они сами появятся здесь.\nНастраивать ничего не нужно: достаточно одной сети (Wi-Fi или LAN)."),
    ("Bu ro'yxatda kompyuter yo'q", "No computers in this list", "В этом списке нет компьютеров"),
    ("Kompyuterni o'ng tugma bilan bosib, \"Xonaga biriktirish\" ni tanlang.", "Right-click a computer and choose \"Assign to room\".", "Щёлкните компьютер правой кнопкой и выберите «Назначить в класс»."),
    ("Kompyuterni qidirish", "Search computers", "Поиск компьютера"),
    ("Ro'yxat", "List", "Список"),
    ("Katakchalar", "Tiles", "Плитки"),
    ("Kattalashtirish (Ctrl + g'ildirak)", "Zoom in (Ctrl + wheel)", "Увеличить (Ctrl + колесо)"),
    ("Kichiklashtirish (Ctrl + g'ildirak)", "Zoom out (Ctrl + wheel)", "Уменьшить (Ctrl + колесо)"),
    ("Qatorda {cols} ta · bosing: 100%", "{cols} per row · click: 100%", "{cols} в ряду · нажмите: 100%"),
    ("Shadow", "Shadow", "Shadow"),
    ("Shadow {pct}%", "Shadow {pct}%", "Shadow {pct}%"),
    (" · Shadow", " · Shadow", " · Shadow"),
    ("Shadow · kutilmoqda", "Shadow · pending", "Shadow · ожидание"),
    ("Shadow · o'chmoqda", "Shadow · turning off", "Shadow · отключается"),
    ("Xizmat", "Service", "Служба"),
    ("Onlayn", "Online", "В сети"),
    ("Kompyuter", "Computer", "Компьютер"),
    ("Holat", "Status", "Состояние"),
    ("Belgilar", "Flags", "Отметки"),
    ("Foydalanuvchi", "User", "Пользователь"),
    ("IP", "IP", "IP"),
    ("Ulangan", "Connected", "Подключён"),
    ("{v0}\nFoydalanuvchi: {v1}\nIP: {v2}\nTizim: {v3}\nXona: {room}\nUlangan: {v4}\nShadow: {v5}\nYangilanishlar: {v6}{v7}{v8}", "{v0}\nUser: {v1}\nIP: {v2}\nSystem: {v3}\nRoom: {room}\nConnected: {v4}\nShadow: {v5}\nUpdates: {v6}{v7}{v8}", "{v0}\nПользователь: {v1}\nIP: {v2}\nСистема: {v3}\nКласс: {room}\nПодключён: {v4}\nShadow: {v5}\nОбновления: {v6}{v7}{v8}"),
    ("Ko'rish", "View", "Смотреть"),
    ("Boshqarish", "Control", "Управлять"),
    ("Xabar", "Message", "Сообщение"),
    ("Nomini o'zgartirish", "Rename", "Переименовать"),
    ("Qulfdan chiqarish", "Unlock", "Разблокировать"),
    ("Yordam berildi", "Helped", "Помощь оказана"),
    ("{v0} daq", "{v0} min", "{v0} мин"),
    ("{v0} soat {v1} daq", "{v0} h {v1} min", "{v0} ч {v1} мин"),
    // --- full-screen viewer --------------------------------------------------------------------
    ("Ro'yxatga qaytish", "Back to the list", "Назад к списку"),
    ("Oldingi kompyuter", "Previous computer", "Предыдущий компьютер"),
    ("Keyingi kompyuter", "Next computer", "Следующий компьютер"),
    ("Boshqaruvni to'xtatish", "Stop control", "Остановить управление"),
    ("Sichqoncha va klaviaturani shu kompyuterga yuborish", "Send mouse and keyboard to this computer", "Передавать мышь и клавиатуру на этот компьютер"),
    ("Ekran kutilmoqda", "Waiting for the screen", "Ожидание экрана"),
    ("Boshqaruv faol · klaviatura shu kompyuterga yuboriladi", "Control active · keyboard goes to this computer", "Управление активно · клавиатура передаётся на этот компьютер"),
    // --- dialog chrome -------------------------------------------------------------------------
    ("Xabarlar", "Messages", "Сообщения"),
    ("Sayt yoki dasturni ochish", "Open a website or program", "Открыть сайт или программу"),
    ("Kompyuterlarni boshqarish", "Power control", "Управление питанием"),
    ("Faollik", "Activity", "Активность"),
    ("Yopish (Esc)", "Close (Esc)", "Закрыть (Esc)"),
    ("{n} ta kompyuterga", "To {n} computers", "На {n} компьютеров"),
    ("Tozalash", "Clear", "Очистить"),
    ("Barcha cheklovlarni olib tashlash", "Remove all restrictions", "Снять все ограничения"),
    ("Ishga tushirish", "Run", "Запустить"),
    ("Bloklash", "Block", "Блокировать"),
    ("Qo'llash", "Apply", "Применить"),
    ("Tasdiqlash", "Confirm", "Подтвердить"),
    ("Saqlash", "Save", "Сохранить"),
    ("Siyosatni saqlash", "Save policy", "Сохранить политику"),
    ("Bekor qilish", "Cancel", "Отмена"),
    // --- journal -------------------------------------------------------------------------------
    ("Diqqat talab qiladi", "Needs attention", "Требует внимания"),
    ("Hodisalar", "Events", "События"),
    ("Hozircha hech narsa yo'q", "Nothing yet", "Пока ничего нет"),
    ("{what} → {n} ta kompyuter", "{what} → {n} computers", "{what} → {n} компьютеров"),
    ("Namoyish boshlandi", "Presentation started", "Показ начат"),
    ("Namoyish to'xtatildi", "Presentation stopped", "Показ остановлен"),
    ("O'qituvchidan xabar", "Message from the teacher", "Сообщение от учителя"),
    ("Dasturlar ro'yxati so'raldi", "Application list requested", "Запрошен список программ"),
    ("Yangilanishlarni tekshirish", "Check for updates", "Проверка обновлений"),
    ("Yangilanishlarni o'rnatish", "Install updates", "Установка обновлений"),
    ("Fayl: {name}", "File: {name}", "Файл: {name}"),
    ("Fayl 100 MB dan katta", "The file is larger than 100 MB", "Файл больше 100 МБ"),
    ("Faylni o'qib bo'lmadi: {e}", "Cannot read the file: {e}", "Не удалось прочитать файл: {e}"),
    ("Dastur bloki olib tashlandi", "Application block removed", "Блокировка программ снята"),
    ("Dasturlar bloklandi", "Applications blocked", "Программы заблокированы"),
    ("NetControl o'chirildi", "NetControl turned off", "NetControl отключён"),
    ("NetControl: {v0} domen, {v1} kalit so'z{v2}", "NetControl: {v0} domains, {v1} keywords{v2}", "NetControl: доменов {v0}, ключевых слов {v1}{v2}"),
    ("Seansdan chiqarish", "Sign out", "Выход из сеанса"),
    ("Shadow yoqish", "Shadow on", "Включение Shadow"),
    ("Shadow o'chirish", "Shadow off", "Отключение Shadow"),
    ("Xizmat rejimi (commit sikli)", "Maintenance (commit cycle)", "Режим обслуживания (цикл фиксации)"),
    ("Saqlanadigan papkalar", "Persistent folders", "Сохраняемые папки"),
    ("Dastur yangilanishlari", "Application updates", "Обновления программ"),
    ("Noto'g'ri kompyuter nomi: {new}", "Invalid computer name: {new}", "Недопустимое имя компьютера: {new}"),
    ("Kompyuter nomi", "Computer name", "Имя компьютера"),
    ("Yangilanish siyosati", "Update policy", "Политика обновлений"),
    ("Vimora yangilanishi", "Vimora update", "Обновление Vimora"),
    ("Chat yakunlandi", "Chat ended", "Чат завершён"),
    ("Tarmoq xatosi: {e}", "Network error: {e}", "Ошибка сети: {e}"),
    ("Konsol tayyor: TCP {port}", "Console ready: TCP {port}", "Консоль готова: TCP {port}"),
    ("{addr}: protokol versiyasi mos emas (v{version})", "{addr}: protocol version mismatch (v{version})", "{addr}: несовместимая версия протокола (v{version})"),
    ("{v0} ulandi ({v1}, {v2})", "{v0} connected ({v1}, {v2})", "{v0} подключён ({v1}, {v2})"),
    ("{v0} yordam so'rayapti: {text}", "{v0} asks for help: {text}", "{v0} просит помощи: {text}"),
    ("{v0} uzildi", "{v0} disconnected", "{v0} отключён"),
    ("Ekranni olish imkonsiz: {e}", "Cannot capture the screen: {e}", "Не удаётся захватить экран: {e}"),
    ("Namoyish to'xtatildi — ekranni olib bo'lmadi: {e}", "Presentation stopped — cannot capture the screen: {e}", "Показ остановлен — не удаётся захватить экран: {e}"),
    ("Salom! Bugun 3-mavzuni boshlaymiz, kitobni oching.", "Hello! Today we start topic 3, open your books.", "Здравствуйте! Сегодня начинаем тему 3, откройте учебники."),
    ("DEV: yangilanishlarni tekshirish", "DEV: check for updates", "DEV: проверка обновлений"),
    // --- launch / file / apps ------------------------------------------------------------------
    ("Sayt", "Website", "Сайт"),
    ("Dastur", "Program", "Программа"),
    ("notepad.exe · C:\\Program Files\\…\\app.exe · firefox", "notepad.exe · C:\\Program Files\\…\\app.exe · firefox", "notepad.exe · C:\\Program Files\\…\\app.exe · firefox"),
    ("Argumentlar (ixtiyoriy)", "Arguments (optional)", "Аргументы (необязательно)"),
    ("Fayl yo'li: C:\\Users\\…\\dars.pdf yoki /home/…/dars.pdf", "File path: C:\\Users\\…\\lesson.pdf or /home/…/lesson.pdf", "Путь к файлу: C:\\Users\\…\\urok.pdf или /home/…/urok.pdf"),
    ("O'quvchining Yuklanmalar/Vimora papkasiga saqlanadi.", "Saved to the student's Downloads/Vimora folder.", "Сохраняется в папку Загрузки/Vimora ученика."),
    ("Jarayon nomlari, har biri yangi qatorda yoki vergul bilan. Ro'yxatdagi dasturlar ochilishi bilan yopiladi.", "Process names, one per line or separated by commas. Listed programs are closed as soon as they start.", "Имена процессов — по одному в строке или через запятую. Программы из списка закрываются сразу после запуска."),
    ("chrome\nsteam.exe\ndiscord", "chrome\nsteam.exe\ndiscord", "chrome\nsteam.exe\ndiscord"),
    // --- power ---------------------------------------------------------------------------------
    ("O'chirish", "Shut down", "Выключить"),
    ("Qayta yoqish", "Restart", "Перезагрузить"),
    ("Seansdan chiqish", "Sign out", "Выйти из сеанса"),
    ("Saqlanmagan ishlar yo'qolishi mumkin!", "Unsaved work may be lost!", "Несохранённая работа может быть потеряна!"),
    // --- Shadow --------------------------------------------------------------------------------
    ("Himoya", "Protection", "Защита"),
    ("Papkalar", "Folders", "Папки"),
    ("Dasturlar", "Applications", "Программы"),
    ("Doim saqlanadigan papkalar", "Always-kept folders", "Всегда сохраняемые папки"),
    ("Bu papkalardagi fayllar qayta yuklanganda o'chmaydi. Har bir papka — yangi qatorda.", "Files in these folders survive a restart. One folder per line.", "Файлы в этих папках не удаляются при перезагрузке. Каждая папка — с новой строки."),
    ("D:\\Darslar\n/home/student/Hujjatlar", "D:\\Lessons\n/home/student/Documents", "D:\\Uroki\n/home/student/Документы"),
    ("Qayta yuklangandan keyin kuchga kiradi.", "Takes effect after a restart.", "Вступает в силу после перезагрузки."),
    ("Kompyuterlar", "Computers", "Компьютеры"),
    ("papka yo'q", "no folders", "нет папок"),
    ("{n} ta papka", "{n} folders", "папок: {n}"),
    ("Tizim yangilanishlari", "System updates", "Обновления системы"),
    ("Avtomatik", "Automatic", "Автоматически"),
    ("Faqat konsoldan", "From the console only", "Только из консоли"),
    ("O'chirilgan", "Off", "Отключено"),
    ("Shadow yoqilgan kompyuterlarda o'rnatilgan yangilanishlar ham saqlanadi.", "On computers with Shadow on, installed updates are kept too.", "На компьютерах с включённым Shadow установленные обновления тоже сохраняются."),
    ("o'chiq", "off", "выкл."),
    ("qayta yuklash kutilmoqda", "restart pending", "ожидает перезагрузки"),
    ("qo'llanmaydi", "not supported", "не поддерживается"),
    ("xizmat yo'q", "no service", "нет службы"),
    ("noma'lum", "unknown", "неизвестно"),
    ("tizim {v0}, dasturlar {v1}", "system {v0}, apps {v1}", "система {v0}, программы {v1}"),
    ("qayta yuklanganda yoqiladi", "turns on after restart", "включится после перезагрузки"),
    ("qayta yuklanganda o'chadi", "turns off after restart", "отключится после перезагрузки"),
    ("xizmat rejimi", "maintenance", "обслуживание"),
    ("Hozirgi holat", "Current state", "Текущее состояние"),
    ("Nima qilamiz?", "What to do?", "Что сделать?"),
    ("Shadow ni yoqish", "Turn Shadow on", "Включить Shadow"),
    ("Shadow ni o'chirish", "Turn Shadow off", "Отключить Shadow"),
    ("Yangilab, qayta yoqish", "Update, then turn back on", "Обновить и включить снова"),
    ("Hozir qayta yuklash", "Restart now", "Перезагрузить сейчас"),
    ("O'zgarish faqat qayta yuklangandan keyin kuchga kiradi.", "The change takes effect only after a restart.", "Изменение вступит в силу только после перезагрузки."),
    ("Vimora xizmati yo'q", "No Vimora service", "Нет службы Vimora"),
    (" · {v0} vazifa", " · {v0} tasks", " · задач: {v0}"),
    ("Siz konsoldan qilgan o'zgarishlar Shadow yoqilgan bo'lsa ham saqlanadi.", "Changes you make from the console are kept even with Shadow on.", "Изменения, сделанные из консоли, сохраняются даже при включённом Shadow."),
    ("Windows: Unified Write Filter (Windows 10/11 Enterprise / Education)\nUbuntu: overlayroot", "Windows: Unified Write Filter (Windows 10/11 Enterprise / Education)\nUbuntu: overlayroot", "Windows: Unified Write Filter (Windows 10/11 Enterprise / Education)\nUbuntu: overlayroot"),
    ("Ro'yxatni olish", "Get the list", "Получить список"),
    ("Dasturni qidirish", "Search applications", "Поиск программы"),
    ("{reported}/{v0} kompyuter ro'yxat yubordi. Belgini olib tashlasangiz, o'sha dastur yangilanmaydi (o'zi ham, Vimora orqali ham).", "{reported}/{v0} computers sent their list. Untick an application to stop its updates (both its own and through Vimora).", "{reported}/{v0} компьютеров прислали список. Снимите отметку, чтобы программа не обновлялась (ни сама, ни через Vimora)."),
    ("Ro'yxat yo'q — \"Ro'yxatni olish\" ni bosing", "No list yet — press \"Get the list\"", "Списка нет — нажмите «Получить список»"),
    ("Hammasi yoqilsin", "All on", "Включить все"),
    ("Hammasi o'chsin", "All off", "Отключить все"),
    ("{v0} ta o'zgarish — \"Qo'llash\" ni bosing", "{v0} changes — press \"Apply\"", "Изменений: {v0} — нажмите «Применить»"),
    // --- rename --------------------------------------------------------------------------------
    ("Konsoldagi nomi", "Name in the console", "Имя в консоли"),
    ("Faqat shu konsolda ko'rinadi, darhol o'zgaradi. Bo'sh qoldirsangiz — tizim nomi ko'rsatiladi.", "Shown only in this console and changes at once. Leave empty to show the system name.", "Видно только в этой консоли, меняется сразу. Если оставить пустым — показывается системное имя."),
    ("Kompyuterning tizim nomini ham o'zgartirish", "Also change the computer's system name", "Изменить и системное имя компьютера"),
    ("Windows'da qayta yuklangandan keyin kuchga kiradi. Shadow rejimda ham saqlanadi (commit).", "On Windows it takes effect after a restart. Kept in Shadow mode too (commit).", "В Windows вступает в силу после перезагрузки. Сохраняется и в режиме Shadow (фиксация)."),
    ("1–15 ta lotin harfi, raqam yoki '-' (faqat raqamdan iborat bo'lmasin).", "1–15 Latin letters, digits or '-' (not digits only).", "1–15 латинских букв, цифр или '-' (не только цифры)."),
    ("Darhol qayta yuklash", "Restart right away", "Сразу перезагрузить"),
    // --- updates dialog ------------------------------------------------------------------------
    ("Avtomatik yangilanish siyosati", "Automatic update policy", "Политика автообновления"),
    ("Windows Update / apt o'zi yuklab, o'zi o'rnatadi.", "Windows Update / apt downloads and installs by itself.", "Windows Update / apt сам скачивает и устанавливает."),
    ("Kompyuterlar o'zi o'rnatmaydi; siz shu yerdan qachon o'rnatishni hal qilasiz.", "Computers don't install by themselves; you decide here when to install.", "Компьютеры не устанавливают сами; вы решаете здесь, когда устанавливать."),
    ("Tizim yangilanishlari butunlay o'chiriladi.", "System updates are turned off completely.", "Обновления системы полностью отключаются."),
    ("Dasturlar o'zi yangilansin (Microsoft Store, Chrome, Edge, snap)", "Let applications update themselves (Microsoft Store, Chrome, Edge, snap)", "Программы обновляются сами (Microsoft Store, Chrome, Edge, snap)"),
    ("Hozir bajarish", "Do it now", "Выполнить сейчас"),
    ("Tizim (Windows Update / apt)", "System (Windows Update / apt)", "Система (Windows Update / apt)"),
    ("Dasturlar (winget / snap)", "Applications (winget / snap)", "Программы (winget / snap)"),
    ("Kerak bo'lsa qayta yuklash", "Restart if needed", "Перезагрузить при необходимости"),
    ("Tekshirish", "Check", "Проверить"),
    ("O'rnatish", "Install", "Установить"),
    ("Shadow rejimdagi kompyuterga yangilanish o'rnatilmaydi (qayta yuklanganda yo'qoladi) — Shadow → \"Xizmat rejimi\" ni ishlating.", "Updates aren't installed on computers in Shadow mode (they would vanish on restart) — use Shadow → \"Maintenance\".", "На компьютеры в режиме Shadow обновления не ставятся (пропадут после перезагрузки) — используйте Shadow → «Обслуживание»."),
    // --- internet ------------------------------------------------------------------------------
    ("Saytlar", "Websites", "Сайты"),
    ("Kalit so'zlar", "Keywords", "Ключевые слова"),
    ("Xavfsiz qidiruv", "Safe search", "Безопасный поиск"),
    ("Tayyor ro'yxatlar", "Ready-made lists", "Готовые списки"),
    ("Bu domenlar va ularning barcha subdomenlari ochilmaydi (hosts fayli + Chrome/Edge/Yandex/Brave siyosatlari).", "These domains and all their subdomains won't open (hosts file + Chrome/Edge/Yandex/Brave policies).", "Эти домены и все их поддомены не откроются (файл hosts + политики Chrome/Edge/Yandex/Brave)."),
    ("Ijtimoiy tarmoqlar", "Social networks", "Соцсети"),
    ("O'yinlar", "Games", "Игры"),
    ("Video", "Video", "Видео"),
    ("Qimor", "Gambling", "Азартные игры"),
    ("Kattalar uchun", "Adult", "Для взрослых"),
    ("youtube.com\ntiktok.com", "youtube.com\ntiktok.com", "youtube.com\ntiktok.com"),
    ("kazino\nreferat yuklab olish", "casino\ndownload essay", "казино\nскачать реферат"),
    ("Google, Yandex, Bing, YouTube va boshqa qidiruv tizimlarida shu so'zlar qidirilsa, brauzer tabi darhol yopiladi, o'quvchiga ogohlantirish chiqadi va sizga xabar keladi. Har biri yangi qatorda; iboralar ham mumkin.", "If one of these words is searched on Google, Yandex, Bing, YouTube or another search engine, the browser tab closes at once, the student sees a warning and you get notified. One per line; phrases are fine.", "Если одно из этих слов ищут в Google, Яндексе, Bing, YouTube или другом поисковике, вкладка браузера сразу закрывается, ученик видит предупреждение, а вам приходит уведомление. По одному в строке; можно фразы."),
    ("Xavfsiz qidiruvni majburlash", "Enforce safe search", "Принудительный безопасный поиск"),
    ("Nomaqbul natijalar qidiruvdan olib tashlanadi; o'quvchi bu sozlamani o'chira olmaydi.", "Inappropriate results are removed from search; students can't turn this off.", "Неприемлемые результаты убираются из поиска; ученик не может это отключить."),
    ("Oilaviy qidiruv", "Family search", "Семейный поиск"),
    ("Cheklangan rejim", "Restricted mode", "Безопасный режим"),
    ("Brauzerlarda DNS-over-HTTPS o'chiriladi, shuning uchun cheklovlarni chetlab bo'lmaydi.", "DNS-over-HTTPS is turned off in browsers, so the restrictions can't be bypassed.", "В браузерах отключается DNS-over-HTTPS, поэтому ограничения нельзя обойти."),
    // --- chat ----------------------------------------------------------------------------------
    ("hech kim", "nobody", "никто"),
    ("{v0}  Kimga: {v1}", "{v0}  To: {v1}", "{v0}  Кому: {v1}"),
    ("Hali xabar yo'q", "No messages yet", "Сообщений пока нет"),
    ("Pastda yozing va Enter bosing", "Type below and press Enter", "Напишите ниже и нажмите Enter"),
    ("Xabar yozing…", "Write a message…", "Напишите сообщение…"),
    ("Enter — yuborish · Shift+Enter — yangi qator", "Enter — send · Shift+Enter — new line", "Enter — отправить · Shift+Enter — новая строка"),
    // --- settings ------------------------------------------------------------------------------
    ("Umumiy", "General", "Общие"),
    ("Ko'rinish", "Appearance", "Внешний вид"),
    ("Qulf ekrani", "Lock screen", "Экран блокировки"),
    ("O'quvchi kompyuterlari", "Student computers", "Компьютеры учеников"),
    ("Yangilanish", "Updates", "Обновление"),
    ("Dastur haqida", "About", "О программе"),
    ("Saqlandi", "Saved", "Сохранено"),
    ("O'zgarishlar darhol saqlanadi", "Changes are saved instantly", "Изменения сохраняются сразу"),
    ("Til", "Language", "Язык"),
    ("Konsol nomi", "Console name", "Имя консоли"),
    ("Konsol tarmoqda shu nom bilan e'lon qilinadi. O'quvchi kompyuterlari jurnalida ham shu nom ko'rinadi.", "The console announces itself on the network under this name. Student computers show it in their logs.", "Под этим именем консоль объявляется в сети. Его же видно в журналах компьютеров учеников."),
    ("Oynani to'liq ochish", "Open maximized", "Открывать развёрнутым"),
    ("Yoqilsa, konsol ekranni to'liq egallab ochiladi. O'chiq bo'lsa — eng kichik o'lchamda (1280 × 720) ochiladi. Keyingi ishga tushirishda kuchga kiradi.", "When on, the console opens filling the screen. When off, it opens at its minimum size (1280 × 720). Takes effect on next start.", "Если включено, консоль открывается на весь экран. Если выключено — в минимальном размере (1280 × 720). Вступает в силу при следующем запуске."),
    ("Mavzu", "Theme", "Тема"),
    ("\"Tizim\" — kompyuteringizdagi qorong'i/yorug' sozlamaga ergashadi.", "\"System\" follows your computer's dark/light setting.", "«Системная» следует настройке тёмной/светлой темы вашего компьютера."),
    ("Qorong'i", "Dark", "Тёмная"),
    ("Yorug'", "Light", "Светлая"),
    ("Kartochka o'lchami", "Card size", "Размер карточек"),
    ("100% — bir qatorda 6 ta kompyuter, 200% — 3 ta. Kompyuterlar sahifasida Ctrl + g'ildirak yoki − / + tugmalari bilan ham o'zgartiriladi.", "100% — 6 computers per row, 200% — 3. Also changed on the computers page with Ctrl + wheel or the − / + buttons.", "100% — 6 компьютеров в ряду, 200% — 3. Также меняется на странице компьютеров через Ctrl + колесо или кнопки − / +."),
    ("Tartiblash", "Sort", "Сортировка"),
    ("IP bo'yicha tartib ko'p xonalarda kompyuterlarning stol ustidagi joylashuviga mos keladi.", "In many rooms, sorting by IP matches how the computers stand on the desks.", "Во многих классах сортировка по IP совпадает с расстановкой компьютеров на столах."),
    ("Nomi bo'yicha", "By name", "По имени"),
    ("IP bo'yicha", "By IP", "По IP"),
    ("Ekran rasmlarini yangilash", "Screen preview refresh", "Обновление превью экранов"),
    ("Kompyuterlar sahifasidagi kichik ekran rasmlari qanchalik tez-tez yangilanadi. Tezroq — tarmoqqa ko'proq yuk.", "How often the small screen previews on the computers page refresh. Faster means more network load.", "Как часто обновляются маленькие превью экранов на странице компьютеров. Чаще — больше нагрузка на сеть."),
    ("{n} s", "{n} s", "{n} с"),
    ("Sarlavha", "Title", "Заголовок"),
    ("Matn", "Text", "Текст"),
    ("Qulflanganda o'quvchi ekranining o'rtasida chiqadi. O'zgarish keyingi qulflashda ko'rinadi.", "Shown in the middle of the student's screen while locked. Changes appear at the next lock.", "Показывается в центре экрана ученика при блокировке. Изменения видны со следующей блокировки."),
    ("Ko'rinishi", "Preview", "Предпросмотр"),
    ("Avtomatik yangilansin", "Update automatically", "Обновлять автоматически"),
    ("O'quvchi kompyuterlari har 6 soatda GitHub'dagi yangi versiyani tekshiradi va o'zi, so'rovsiz o'rnatadi. Shadow yoqilgan kompyuterlar o'tkazib yuboriladi. O'zgarish hozir onlayn bo'lgan kompyuterlarga yuboriladi.", "Student computers check GitHub for a new version every 6 hours and install it silently. Computers with Shadow on are skipped. The change is sent to the computers that are online now.", "Компьютеры учеников каждые 6 часов проверяют новую версию на GitHub и устанавливают её без вопросов. Компьютеры с включённым Shadow пропускаются. Изменение отправляется компьютерам, которые сейчас в сети."),
    ("Hozir yangilash", "Update now", "Обновить сейчас"),
    ("Onlayn kompyuterlar so'nggi versiyani hozir yuklab oladi va o'rnatadi. Agent o'zi qayta ishga tushadi.", "Online computers download and install the latest version now. The agent restarts by itself.", "Компьютеры в сети сейчас скачают и установят последнюю версию. Агент перезапустится сам."),
    ("{n} ta kompyuter", "{n} computers", "{n} компьютеров"),
    ("Ulanish", "Connection", "Подключение"),
    ("Konsol identifikatori", "Console ID", "Идентификатор консоли"),
    ("Vimora o'rnatilgan kompyuterlar tarmoqda o'zi paydo bo'ladi. Har bir kompyuter birinchi ulangan konsolni eslab qoladi va faqat unga bo'ysunadi — boshqa birov o'z konsolini ochib boshqara olmaydi. Konsolni almashtirish uchun o'quvchi kompyuterida: vimora-client unpair", "Computers with Vimora appear on the network by themselves. Each one remembers the first console it connects to and obeys only that console — nobody else can take over by opening their own. To switch consoles, run on the student computer: vimora-client unpair", "Компьютеры с Vimora сами появляются в сети. Каждый запоминает первую консоль, к которой подключился, и подчиняется только ей — другой человек не сможет управлять, открыв свою консоль. Чтобы сменить консоль, на компьютере ученика выполните: vimora-client unpair"),
    ("Kompyuterlarni xonalarga kompyuterlar sahifasida o'ng tugma orqali taqsimlaysiz.", "You assign computers to rooms with right-click on the computers page.", "Компьютеры распределяются по классам правым кликом на странице компьютеров."),
    ("Konsol versiyasi", "Console version", "Версия консоли"),
    ("Hali tekshirilmagan", "Not checked yet", "Ещё не проверялось"),
    ("Tekshirilmoqda…", "Checking…", "Проверка…"),
    ("Tekshirib bo'lmadi", "Couldn't check", "Не удалось проверить"),
    ("Eng so'nggi versiya o'rnatilgan", "You have the latest version", "Установлена последняя версия"),
    ("Yangi versiya mavjud: {v}", "New version available: {v}", "Доступна новая версия: {v}"),
    ("{v} yuklab olinmoqda — {pct}%", "Downloading {v} — {pct}%", "Загрузка {v} — {pct}%"),
    ("{v} o'rnatilmoqda…", "Installing {v}…", "Установка {v}…"),
    ("{v} o'rnatildi — qayta ishga tushiring", "{v} installed — restart to use it", "{v} установлена — перезапустите"),
    ("Vimora {v}", "Vimora {v}", "Vimora {v}"),
    ("{v} ni o'rnatish", "Install {v}", "Установить {v}"),
    ("Qayta ishga tushirish", "Restart", "Перезапустить"),
    ("Avtomatik tekshirish", "Check automatically", "Проверять автоматически"),
    ("Konsol ishga tushganda va har 6 soatda GitHub'dan yangi versiyani tekshiradi. Yangilanish faqat siz \"O'rnatish\" tugmasini bosganingizda o'rnatiladi.", "Checks GitHub for a new version when the console starts and every 6 hours. An update is installed only when you press \"Install\".", "Проверяет новую версию на GitHub при запуске консоли и каждые 6 часов. Обновление устанавливается только когда вы нажмёте «Установить»."),
    ("Relizlar sahifasi", "Releases page", "Страница релизов"),
    ("Har bir yuklab olingan fayl SHA-256 nazorat yig'indisi bilan tekshiriladi.", "Every downloaded file is verified with its SHA-256 checksum.", "Каждый загруженный файл проверяется по контрольной сумме SHA-256."),
    ("Versiya {v} · MIT litsenziyasi", "Version {v} · MIT license", "Версия {v} · лицензия MIT"),
    ("Kompyuter xonalarini boshqarish uchun ochiq kodli dastur: ekranlarni kuzatish, qulflash, namoyish, chat, internet nazorati va Shadow rejim.", "Open-source classroom management: watch screens, lock, present, chat, control the internet and Shadow mode.", "Программа с открытым кодом для управления компьютерными классами: наблюдение за экранами, блокировка, показ, чат, контроль интернета и режим Shadow."),
    ("Sozlamalar papkasi", "Settings folder", "Папка настроек"),
    ("server.json (sozlamalar) va identity.key (konsol kaliti) shu yerda. identity.key yo'qolsa, kompyuterlarni qaytadan bog'lash kerak bo'ladi.", "server.json (settings) and identity.key (the console key) live here. If identity.key is lost, the computers have to be paired again.", "Здесь лежат server.json (настройки) и identity.key (ключ консоли). Если identity.key потеряется, компьютеры придётся привязать заново."),
    ("Manba kodi", "Source code", "Исходный код"),
];

#[cfg(test)]
mod tests {
    use super::ROWS;

    /// Every placeholder of the Uzbek text must exist in both translations.
    #[test]
    fn placeholders_match() {
        let holes = |s: &str| -> Vec<String> {
            let mut v: Vec<String> = s
                .split('{')
                .skip(1)
                .filter_map(|p| p.split_once('}').map(|(h, _)| h.to_string()))
                .collect();
            v.sort();
            v
        };
        for (uz, en, ru) in ROWS {
            assert_eq!(holes(uz), holes(en), "{uz}");
            assert_eq!(holes(uz), holes(ru), "{uz}");
        }
    }

    /// Every `tr("…")` / `tf!("…")` in the console's source has a row.
    #[test]
    fn all_strings_translated() {
        let keys: std::collections::HashSet<&str> = ROWS.iter().map(|r| r.0).collect();
        let mut missing = vec![];
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut files = vec![std::path::PathBuf::from(dir)];
        while let Some(p) = files.pop() {
            if p.is_dir() {
                files.extend(std::fs::read_dir(&p).unwrap().map(|e| e.unwrap().path()));
                continue;
            }
            if p.extension().is_none_or(|e| e != "rs") || p.ends_with("i18n_table.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&p).unwrap();
            for marker in ["tr(\"", "tf!(\""] {
                for (i, _) in src.match_indices(marker) {
                    let rest = &src[i + marker.len()..];
                    let mut lit = String::new();
                    let mut chars = rest.chars();
                    while let Some(c) = chars.next() {
                        match c {
                            '"' => break,
                            '\\' => match chars.next() {
                                Some('n') => lit.push('\n'),
                                Some(o) => lit.push(o),
                                None => break,
                            },
                            c => lit.push(c),
                        }
                    }
                    if !keys.contains(lit.as_str()) {
                        missing.push(format!("{}: {lit:?}", p.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "untranslated:\n{}", missing.join("\n"));
    }

    #[test]
    fn no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for (uz, _, _) in ROWS {
            assert!(seen.insert(uz), "duplicate row: {uz}");
        }
    }
}
