# Профилирование ядер Linux

[Сводные таблицы и выводы](summary.md)

- [linux-3.4.5](linux-3.4.5/table.md)
- [linux-3.4.6](linux-3.4.6/table.md)
- [linux-3.4.7](linux-3.4.7/table.md)

Все 60 замеров собраны, включая 12 L3. Таблицы обновлены по отчётам пользователя.
Проверка: [validation.txt](validation.txt). L3 включает активность всей системы.

Для повторного пересчёта таблиц:
```bash
python3 scripts/benchmark_kernel_cache.py --report /home/vlm326/Documents/Projects/Rust/hash_combination/artifacts/cache_info/runs/2026-10-29_12-12-32_kernels
```

Карты TSV, логи и снимок рабочего diff остаются локально и не входят в Git. Карты пересоздаются командами подготовки; входы определяются размерами и SHA-256 в метаданных.
