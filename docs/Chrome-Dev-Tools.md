# **Chrome DevTools Functional Testing Guide**

## **Navigation and Inspection**

Start your functional testing process by loading the target web page. Inspect the rendered page elements to verify the overall document structure. Ensure that HTML tags and element attributes match the expected values. Use the Chrome DevTools protocol to automate this basic inspection process.

```JSON
{  
  "method": "browser\_tool.execute\_cdp\_command",  
  "params": {  
    "command": "Page.navigate",  
    "args": {  
      "url": "\[https://example.com\](https://example.com)"  
    }  
  }  
}
```

## **Network Validation and Throttling**

Monitor the network traffic to validate API calls and server responses. Check the HTTP status codes and payload data for system errors. Simulate slow network connections to test application performance under heavy stress. This testing practice ensures the application handles poor network connectivity correctly.

```JSON
{  
  "method": "browser\_tool.execute\_cdp\_command",  
  "params": {  
    "command": "Network.emulateNetworkConditions",  
    "args": {  
      "offline": false,  
      "latency": 200,  
      "downloadThroughput": 50000,  
      "uploadThroughput": 50000  
    }  
  }  
}
```

## **Storage Management**

Modern applications store user session state in cookies and local storage. Test these stored data values to ensure correct user session management. Clear the browser data storage between tests to maintain clean states. You can retrieve active cookies directly through the provided browser protocol.

```JSON
{  
  "method": "browser\_tool.execute\_cdp\_command",  
  "params": {  
    "command": "Network.getCookies",  
    "args": {  
      "urls": \["\[https://example.com\](https://example.com)"\]  
    }  
  }  
}
```

## **Console Monitoring**

The standard browser console records JavaScript errors and active system warnings. Monitor this console output to catch unhandled code exceptions during testing. A completely clean console log indicates stable client-side web application execution. Enable the log domain to capture these critical browser messages automatically.

```JSON
{  
  "method": "browser\_tool.execute\_cdp\_command",  
  "params": {  
    "command": "Log.enable",  
    "args": {}  
  }  
}
```

## **Accessibility Testing**

Modern web applications must serve users with varying physical accessibility needs. Inspect the browser accessibility tree to verify basic screen reader compatibility. Check the element attributes to ensure proper structural and semantic meaning. Fetch the full accessibility tree to validate these core accessibility requirements.

```JSON
{  
  "method": "browser\_tool.execute\_cdp\_command",  
  "params": {  
    "command": "Accessibility.getFullAXTree",  
    "args": {}  
  }  
}
```

## **Basic Functional Workflow**

To test basic functionality, navigate to the target web application. Next, locate a specific interactive element like a main button. Simulate a user click to trigger the expected application behavior. Verify the resulting document changes to confirm the action succeeded. You can execute these actions directly using runtime evaluation commands.

```JSON
{  
  "method": "browser\_tool.execute\_cdp\_command",  
  "params": {  
    "command": "Runtime.evaluate",  
    "args": {  
      "expression": "document.querySelector('button').click()"  
    }  
  }  
}
```

## **Visual Design Verification**

Compare the rendered page against original design files. Capture full page screenshots to verify visual layout accuracy. Inspect specific element nodes to check exact pixel dimensions. Use the browser protocol to capture and download screenshots. Compare these generated images to design mockups to find discrepancies.

```JSON
{  
  "method": "browser\_tool.execute\_cdp\_command",  
  "params": {  
    "command": "Page.captureScreenshot",  
    "args": {  
      "format": "png",  
      "captureBeyondViewport": true  
    }  
  }  
}  
```