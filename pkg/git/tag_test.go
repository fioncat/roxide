package git

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestApplyTag(t *testing.T) {
	tests := []struct {
		name string

		tag  string
		rule string

		want string
	}{
		{
			name: "Test patch rule",

			tag:  "v1.2.3",
			rule: "v{0}.{1}.{2+}",

			want: "v1.2.4",
		},
		{
			name: "Test minor rule",

			tag:  "v1.2.3",
			rule: "v{0}.{1+}.{2}",

			want: "v1.3.3",
		},
		{
			name: "Test major rule",

			tag:  "v1.2.3",
			rule: "v{0+}.{1}.{2}",

			want: "v2.2.3",
		},
		{
			name: "Test out of range",

			tag:  "v1.2.3",
			rule: "v{0}.{1}.{3+}",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tag := Tag(tt.tag)
			newTag, err := tag.ApplyRule(tt.rule)
			if tt.want == "" {
				assert.NotNil(t, err)
				return
			}
			assert.NoError(t, err)
			assert.Equal(t, tt.want, string(newTag))
		})
	}
}
