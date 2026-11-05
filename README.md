# Vimora

**Kompyuter xonalarini boshqarish uchun bepul va ochiq dastur.** NetSupport School va Veyon'ga
muqobil. O'qituvchi bitta kompyuterdan sinfdagi barcha kompyuterlarni ko'radi va boshqaradi.

Rust'da yozilgan, tez ishlaydi. **Windows** va **Ubuntu** uchun.

![Vimora](docs/dark.jpg)

---

## Nima qila oladi

| | |
|---|---|
| 🖥 **Kuzatish** | Barcha ekranlar bir joyda (katakcha yoki ro'yxat ko'rinishi), bittasini katta ochish |
| 🖱 **Boshqarish** | O'quvchi kompyuterini masofadan sichqoncha va klaviatura bilan boshqarish |
| 🔒 **Qulflash** | Dars vaqtida ekranni qulflash — o'quvchi chalg'imaydi |
| 📽 **Namoyish** | O'qituvchi ekranini hamma o'quvchilarga ko'rsatish |
| 💬 **Xabarlar** | Telegram'ga o'xshash chat — o'quvchi javob yoza oladi |
| 🌐 **Internet** | Saytlarni bloklash, taqiqlangan so'zlarni qidirishni to'xtatish, xavfsiz qidiruv |
| 🚫 **Dasturlar** | Keraksiz dasturlarni bloklash, sayt yoki dasturni masofadan ochish, fayl yuborish |
| ❄️ **Shadow rejim** | Qayta yuklanganda kompyuter "yangidek" bo'lib yonadi (Shadow Defender / Deep Freeze kabi) |
| ⬇️ **Yangilanishlar** | Windows Update va dasturlar yangilanishini o'qituvchi kompyuteridan boshqarish |
| ⚡ **Quvvat** | O'chirish, qayta yoqish, seansdan chiqarish |
| 🏫 **Xonalar** | Kompyuterlarni xonalarga ajratish, nomini o'zgartirish |

Sichqoncha bilan belgilash (File Explorer kabi), qorong'i va yorug' mavzu, Lucide ikonlari.

| Yorug' mavzu | Ro'yxat ko'rinishi |
|---|---|
| ![](docs/light.jpg) | ![](docs/list.jpg) |

| Shadow rejim | Chat (o'qituvchi) | Chat (o'quvchi) |
|---|---|---|
| ![](docs/shadow.jpg) | ![](docs/chat.jpg) | ![](docs/chat-student.jpg) |

---

## Qanday ishlaydi

Ikki qism bor:

- **`vimora-server`** — o'qituvchi kompyuteridagi konsol.
- **`vimora-client`** — har bir o'quvchi kompyuteridagi agent.

O'quvchi kompyuterlari konsolni tarmoqda (Wi-Fi yoki LAN) o'zi topadi va ulanadi. Ular bir xil
**ulanish kaliti** bilan bir-birini taniydi — kalitni bilmagan odam kompyuterlarni boshqara olmaydi.

---

## O'rnatish

### Windows

```powershell
powershell -ExecutionPolicy Bypass -File installer\windows\build.ps1
```

`dist\` papkasida o'rnatuvchi paydo bo'ladi. O'rnatishda **O'qituvchi** yoki **O'quvchi**
kompyuterini tanlaysiz. O'quvchi uchun ulanish kalitini kiritasiz — qolganini o'rnatuvchi o'zi qiladi.

### Ubuntu

```bash
installer/linux/package.sh
tar xf dist/vimora-*-linux-x86_64.tar.gz && cd vimora-*-linux-x86_64
sudo ./install.sh
```

`1` — o'qituvchi, `2` — o'quvchi. O'chirish: `sudo ./uninstall.sh`.

---

## Ishlab chiquvchilar uchun

[Rust](https://rustup.rs) kerak. Linux'da qo'shimcha paket shart emas.

```bash
cargo run -p vimora-server                                 # konsol
cargo run -p vimora-client -- --server 127.0.0.1:47801     # agent (sinov uchun)
cargo test --workspace
```

```
crates/vimora-core   protokol, kalit bilan himoyalangan ulanish, ekran olish
crates/vimora-ui     dizayn: ranglar, shriftlar, ikonlar
apps/vimora-server   o'qituvchi konsoli
apps/vimora-client   o'quvchi agenti + Vimora xizmati (Shadow, yangilanishlar, internet)
installer/           Windows va Ubuntu o'rnatuvchilari
```

---

## Bilish kerak

- **Shadow rejim** Windows'da faqat **Enterprise** yoki **Education** nashrida ishlaydi
  (Home/Pro'da UWF yo'q). Ubuntu'da `overlayroot` orqali ishlaydi.
- Ubuntu'da ekranni ko'rish va boshqarish **X11** seansida ishlaydi (Wayland'da emas). O'rnatuvchi buni sozlab beradi.
- Ulanish kalit bilan tekshiriladi, lekin trafik shifrlanmaydi — faqat ishonchli lokal tarmoqda ishlating.
- Portlar: o'qituvchi — **TCP 47801**, o'quvchi — **UDP 47800**.
- Loyiha yangi: real sinfda sinab ko'rilmoqda. Xato topsangiz, [Issue](https://github.com/veymay/vimora/issues) oching.

---

## Litsenziya

[MIT](LICENSE) — bepul foydalaning, o'zgartiring, tarqating.

Ikonlar: [Lucide](https://lucide.dev) (ISC). Shriftlar: [Geist](https://vercel.com/font) (OFL).
