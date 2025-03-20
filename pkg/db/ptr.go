package db

func StringPtr(s string) *string {
	return &s
}

func Uint64Ptr(i uint64) *uint64 {
	return &i
}

func IntPtr(i int) *int {
	return &i
}

func BoolPtr(b bool) *bool {
	return &b
}
