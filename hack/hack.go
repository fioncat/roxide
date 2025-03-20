package hack

import (
	_ "embed"
	"strings"
)

//go:embed wrap.sh
var wrap string

func GetWrap(name, binary string) string {
	wrap = strings.ReplaceAll(wrap, "{{name}}", name)
	wrap = strings.ReplaceAll(wrap, "{{binary}}", binary)
	return wrap
}
