!function(){var e;function t(e){return new EventSource(e,{withCredentials:!0})}function n(t){if(e.getAttributeValue(t,"sse-swap")){if(null==(u=e.getClosestMatch(t,i)))return null;for(var n=e.getInternalData(u).sseEventSource,r=e.getAttributeValue(t,"sse-swap").split(","),o=0;o<r.length;o++){const i=r[o].trim(),c=function(r){s(u)||(e.bodyContains(t)?e.triggerEvent(t,"htmx:sseBeforeMessage",r)&&(a(t,r.data),e.triggerEvent(t,"htmx:sseMessage",r)):n.removeEventListener(i,c))};e.getInternalData(t).sseEventListener=c,n.addEventListener(i,c)}}if(e.getAttributeValue(t,"hx-trigger")){var u;if(null==(u=e.getClosestMatch(t,i)))return null;n=e.getInternalData(u).sseEventSource;e.getTriggerSpecs(t).forEach((function(r){if("sse:"===r.trigger.slice(0,4)){var a=function(i){s(u)||(e.bodyContains(t)||n.removeEventListener(r.trigger.slice(4),a),htmx.trigger(t,r.trigger,i),htmx.trigger(t,"htmx:sseMessage",i))};e.getInternalData(t).sseEventListener=a,n.addEventListener(r.trigger.slice(4),a)}}))}}function r(t,a){if(null==t)return null;if(e.getAttributeValue(t,"sse-connect")){var i=e.getAttributeValue(t,"sse-connect");if(null==i)return;!function(t,a,i){var o=htmx.createEventSource(a);o.onerror=function(n){if(e.triggerErrorEvent(t,"htmx:sseError",{error:n,source:o}),!s(t)&&o.readyState===EventSource.CLOSED){i=i||0;var a=500*(i=Math.max(Math.min(2*i,128),1));window.setTimeout((function(){r(t,i)}),a)}},o.onopen=function(r){if(e.triggerEvent(t,"htmx:sseOpen",{source:o}),i&&i>0){const e=t.querySelectorAll("[sse-swap], [data-sse-swap], [hx-trigger], [data-hx-trigger]");for(let t=0;t<e.length;t++)n(e[t]);i=0}},e.getInternalData(t).sseEventSource=o;var u=e.getAttributeValue(t,"sse-close");u&&o.addEventListener(u,(function(){o.close()}))}(t,i,a)}n(t)}function s(t){if(!e.bodyContains(t)){var n=e.getInternalData(t).sseEventSource;if(null!=n)return n.close(),!0}return!1}function a(t,n){e.withExtensions(t,(function(e){n=e.transformResponse(n,null,t)}));var r=e.getSwapSpecification(t),s=e.getTarget(t);e.swap(s,n,r)}function i(t){return null!=e.getInternalData(t).sseEventSource}htmx.defineExtension("sse",{init:function(n){e=n,null==htmx.createEventSource&&(htmx.createEventSource=t)},getSelectors:function(){return["[sse-connect]","[data-sse-connect]","[sse-swap]","[data-sse-swap]"]},onEvent:function(t,n){var s=n.target||n.detail.elt;switch(t){case"htmx:beforeCleanupElement":var a=e.getInternalData(s);return void(a.sseEventSource&&a.sseEventSource.close());case"htmx:afterProcessNode":r(s)}}})}();
