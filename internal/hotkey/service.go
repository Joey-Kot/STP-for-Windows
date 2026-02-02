package hotkey

type EventType int

const (
	TaskEvent EventType = iota + 1
	StopEvent
)

type Event struct {
	Type   EventType
	TaskID int
}

type Service interface {
	Start(handler func(Event)) error
	Close() error
}

type Options struct {
	UseHook        bool
	TaskHotkeys    map[int]string
	StopTaskHotkey string
	Debug          bool
}

func NewService(opts Options) Service {
	return newPlatformService(opts)
}
