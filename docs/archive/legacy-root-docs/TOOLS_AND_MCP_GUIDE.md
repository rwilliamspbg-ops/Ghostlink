# 🛠️ TOOLS & MCP SERVERS - GHOSTLINK STUDIO

## Overview

Your Ghostlink Studio now supports comprehensive tool integration and MCP (Model Context Protocol) servers directly in the Chat interface.

---

## 🔧 Available Built-in Tools

### 1. **web_search**
Search the web for information
- Use for: Latest news, real-time data, web information
- Returns: Search results with URLs and summaries

### 2. **calculator**
Perform mathematical calculations
- Use for: Complex math, unit conversions, computations
- Returns: Calculation results

### 3. **code_execution**
Execute Python code safely
- Use for: Running Python scripts, data processing, automation
- Returns: Code output and results
- **Note**: Sandboxed execution environment

### 4. **file_operations**
Read and write files
- Use for: File management, document processing
- Operations: Read, write, delete, list files
- **Note**: Respects file system permissions

### 5. **terminal**
Execute terminal commands
- Use for: System operations, running scripts
- Returns: Command output
- **Note**: Limited to safe commands, sandboxed

### 6. **database_query**
Query databases
- Use for: Data retrieval, analytics, reporting
- Supports: SQL queries on configured databases
- Returns: Query results in structured format

### 7. **api_call**
Make HTTP API calls
- Use for: Integrating with external APIs
- Methods: GET, POST, PUT, DELETE
- Returns: API response

### 8. **image_generation**
Generate and manipulate images
- Use for: Creating images from descriptions
- Features: Image generation, editing, enhancement
- Returns: Generated image

---

## 🌐 MCP Servers

### What are MCP Servers?

MCP (Model Context Protocol) servers extend your model's capabilities by connecting to remote services that provide additional tools and data access.

### Adding an MCP Server

1. **In Chat Tab**, find "Tools & MCP" section
2. Click **Show** to expand
3. Click **Add** in the MCP Servers section
4. Fill in:
   - **Server name**: Friendly name (e.g., "Weather API")
   - **Server URL**: Full URL (e.g., `http://localhost:5000`)
5. Click **Add**
6. Toggle the checkbox to enable/disable

### Example MCP Servers

```
Name: Weather Service
URL: http://localhost:5001
Tools: get_weather, forecast

Name: Database Tools
URL: http://192.168.1.100:5000
Tools: query_db, get_schema

Name: File Management
URL: http://localhost:5002
Tools: list_files, read_file, write_file
```

### Setting Up a Local MCP Server

Create a simple Python MCP server:

```python
from flask import Flask, jsonify

app = Flask(__name__)

@app.route('/tools', methods=['GET'])
def get_tools():
    return jsonify({
        'tools': [
            {
                'name': 'custom_tool',
                'description': 'Your custom tool',
                'parameters': {
                    'query': {'type': 'string', 'description': 'Input query'}
                }
            }
        ]
    })

@app.route('/execute', methods=['POST'])
def execute():
    # Your tool logic here
    return jsonify({'result': 'Tool output'})

if __name__ == '__main__':
    app.run(port=5000)
```

---

## 💬 Using Tools & MCP in Chat

### Simple Example 1: Web Search

1. Select a model
2. In "Tools & MCP", enable **web_search**
3. Type message: "What's the latest AI news?"
4. Send
5. Model uses web_search tool to find current information
6. Response includes information from web search

### Simple Example 2: Calculator

1. Enable **calculator** tool
2. Type message: "Calculate 156 * 3.14 / 2"
3. Send
4. Model uses calculator tool
5. Response with result

### Advanced Example: Multiple Tools

1. Enable: **web_search**, **code_execution**, **file_operations**
2. Type message: "Search for current Bitcoin price, calculate ROI for $1000 investment at different price points, and save results to a file"
3. Model orchestrates multiple tools:
   - Uses web_search for current price
   - Uses code_execution for calculations
   - Uses file_operations to save results

### MCP Server Integration

1. Start your MCP server: `python mcp_server.py`
2. Add MCP Server in Chat:
   - Name: "My Tools"
   - URL: "http://localhost:5000"
3. Enable the server
4. Use tools from the MCP server in your prompts

---

## 🔌 How It Works

### Request Flow

```
User Message
    ↓
Enable Tools/MCP ← (Selected in Chat UI)
    ↓
Send to Model
    ↓
Model Analyzes
    ↓
Model Decides Which Tools to Use
    ↓
Execute Tools (Parallel)
    ↓
Gather Results
    ↓
Model Processes Results
    ↓
Generate Final Response
    ↓
Display to User (with tools_used)
```

### Tool Configuration in Payload

When you send a message with tools:

```json
{
  "message": "User message",
  "tools": ["web_search", "calculator"],
  "mcp_servers": [
    {
      "name": "My MCP Server",
      "url": "http://localhost:5000"
    }
  ]
}
```

---

## 📊 Response Format

When tools are used, response includes:

```
Assistant: [Response with tool results]

Request: <request_id>

Tools used: web_search, calculator
```

---

## 🔐 Security Considerations

### Safe Execution
- All tools run in sandboxed environments
- File operations limited to designated directories
- Terminal commands restricted to safe subset
- API calls validated and rate-limited

### MCP Server Security
- Verify MCP server URLs before adding
- Use HTTPS for remote servers
- Authenticate if required
- Monitor tool usage

### Best Practices
1. Start with built-in tools
2. Test MCP servers in development first
3. Don't expose sensitive data in prompts
4. Use appropriate system prompts
5. Monitor tool execution results

---

## 🚀 Advanced Usage

### Chaining Tools

```
Prompt: "Find current software engineer salaries, 
analyze trends, compare with cost of living, 
generate report"

Tools: web_search → code_execution → file_operations
```

### Conditional Tool Usage

Model automatically chooses tools based on task:
- Data question → web_search, database_query
- Calculation → calculator, code_execution
- File task → file_operations
- Integration → api_call, mcp_servers

### Tool Limitations & Timeouts

- Web search: 30 second timeout
- Code execution: 60 second timeout, 512MB memory
- File operations: 1GB size limit
- API calls: 30 second timeout
- Custom MCP: Depends on server

---

## 🆘 Troubleshooting

### MCP Server Not Connecting
```
1. Verify server is running: curl http://localhost:5000
2. Check URL format: http://host:port (no trailing slash)
3. Check firewall/network connectivity
4. Restart server and try again
```

### Tool Not Executing
```
1. Verify tool is enabled (checkbox checked)
2. Check tool requirements (e.g., API keys, permissions)
3. Review error in response
4. Check backend logs
```

### MCP Server Errors
```
1. Verify MCP server implements correct protocol
2. Check server URL and port
3. Test with curl: curl -X GET http://server/tools
4. Review server logs for errors
```

---

## 📚 Common Tool Combinations

### Content Research
- **web_search** → Find information
- **file_operations** → Save results
- **code_execution** → Process data

### Data Analysis
- **api_call** → Fetch data
- **code_execution** → Analyze
- **file_operations** → Export

### Development Assistance
- **code_execution** → Run code
- **terminal** → Execute commands
- **file_operations** → Manage files
- **web_search** → Find solutions

### Creative Tasks
- **web_search** → Research
- **image_generation** → Create visuals
- **code_execution** → Process
- **file_operations** → Save

---

## 🔄 Workflow Examples

### Example 1: Market Research
```
1. Enable: web_search, code_execution
2. Prompt: "Research AI market size in 2024, compare with 2023, project 2025"
3. Model:
   - Uses web_search to find market data
   - Uses code_execution to calculate growth
   - Returns analysis with sources
```

### Example 2: System Automation
```
1. Enable: terminal, file_operations, code_execution
2. Prompt: "Check disk usage, identify large files, generate cleanup script"
3. Model:
   - Uses terminal for disk info
   - Uses file_operations to list files
   - Uses code_execution to generate script
```

### Example 3: Custom Integration
```
1. Add MCP Server: "Weather" at http://localhost:5001
2. Enable: web_search, custom MCP tools
3. Prompt: "Plan weekend trip considering weather, cost, activities"
4. Model:
   - Uses MCP weather tool
   - Uses web_search for activities/costs
   - Provides recommendation
```

---

## 📝 API Integration

### Sending Tools in API Call

```bash
curl -X POST http://localhost:8003/api/inference/chat \
  -H "Content-Type: application/json" \
  -d '{
    "message": "Your prompt here",
    "tools": ["web_search", "calculator"],
    "mcp_servers": [
      {
        "name": "My MCP",
        "url": "http://localhost:5000"
      }
    ]
  }'
```

### Expected Response

```json
{
  "response": "Model response with tool results",
  "request_id": "req_123",
  "tools_used": ["web_search", "calculator"]
}
```

---

## ✅ Checklist

- [ ] Review available built-in tools
- [ ] Enable tools for your use case
- [ ] Test individual tools first
- [ ] Add MCP servers if needed
- [ ] Test tool combinations
- [ ] Monitor tool execution
- [ ] Review security settings
- [ ] Use in production

---

## 🎯 Next Steps

1. **Enable Tools**: Check boxes for tools you need
2. **Test**: Send a message using the tools
3. **Add MCP**: If you have external services
4. **Monitor**: Check "Tools used" in responses
5. **Optimize**: Adjust tools for your workflow

---

**Tools & MCP servers amplify your model's capabilities! 🚀**

Start by enabling web_search and calculator, then expand as needed.
