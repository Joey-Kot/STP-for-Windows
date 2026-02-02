package request

type BuildInput struct {
	Model       string
	Temperature float64
	MaxTokens   int
	Prompt      string
	UserText    string
	Extra       map[string]interface{}
}

func BuildPayload(in BuildInput) map[string]interface{} {
	payload := make(map[string]interface{})
	if in.Model != "" {
		payload["model"] = in.Model
	}
	payload["messages"] = []map[string]string{
		{"role": "developer", "content": in.Prompt},
		{"role": "user", "content": in.UserText},
	}
	if in.MaxTokens > 0 {
		payload["max_tokens"] = in.MaxTokens
	}
	payload["temperature"] = in.Temperature
	for k, v := range in.Extra {
		payload[k] = v
	}
	return StripEmptyFields(payload)
}
