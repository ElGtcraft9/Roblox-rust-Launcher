#!/bin/bash
# Mejoras de rendimiento para jugar con Sober en un APU AMD (Bazzite/Fedora).
# Necesita administrador y es reversible:
#   sudo ./rendimiento-linux.sh           activa
#   sudo ./rendimiento-linux.sh --restaurar
# 1. GPU: reloj alto fijo (evita que se quede en frecuencias bajas y los
#    fotogramas se redondeen a 30).
# 2. CPU: preferencia de energía «rendimiento».
# 3. Planificador scx_lavd (pensado para juegos), si está instalado.
set -u
[ "$(id -u)" = 0 ] || { echo "Ejecútalo con sudo."; exit 1; }
if [ "${1:-}" = "--restaurar" ]; then
    for f in /sys/class/drm/card*/device/power_dpm_force_performance_level; do [ -w "$f" ] && echo auto > "$f"; done
    for f in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do [ -w "$f" ] && echo balance_performance > "$f"; done
    command -v scxctl >/dev/null && scxctl stop 2>/dev/null
    echo "Restaurado."; exit 0
fi
for f in /sys/class/drm/card*/device/power_dpm_force_performance_level; do
    [ -w "$f" ] && echo high > "$f" && echo "GPU en rendimiento alto: $f"
done
for f in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do
    [ -w "$f" ] && echo performance > "$f"
done
echo "CPU en preferencia «performance»."
if command -v scxctl >/dev/null; then
    scxctl start --sched lavd --mode gaming 2>/dev/null && echo "Planificador scx_lavd activo." || echo "scx_lavd no se pudo iniciar (opcional)."
fi
echo "Listo. Estos cambios se pierden al reiniciar. Para deshacerlos ya: sudo $0 --restaurar"
