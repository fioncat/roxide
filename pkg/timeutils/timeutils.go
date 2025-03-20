package timeutils

import (
	"fmt"
	"time"
)

const (
	MinuteSeconds = 60
	HourSeconds   = 60 * MinuteSeconds
	DaySeconds    = 24 * HourSeconds
	WeekSeconds   = 7 * DaySeconds
	YearSeconds   = 365 * DaySeconds
)

func Now() uint64 {
	return (uint64)(time.Now().Unix())
}

func FormatSince(time, now uint64) string {
	if time == 0 {
		return "never"
	}
	var delta uint64
	var style string
	if now > time {
		delta = now - time
		style = "ago"
	} else {
		delta = time - now
		style = "left"
	}

	var unit string
	var value uint64
	switch {
	case delta < MinuteSeconds:
		unit = "s"
		if delta < 30 {
			return "now"
		}
		value = delta

	case delta < HourSeconds:
		unit = "m"
		value = delta / MinuteSeconds

	case delta < DaySeconds:
		unit = "h"
		value = delta / HourSeconds

	case delta < YearSeconds:
		unit = "d"
		value = delta / DaySeconds

	default:
		unit = "y"
		value = delta / YearSeconds
	}

	return fmt.Sprintf("%d%s %s", value, unit, style)
}
