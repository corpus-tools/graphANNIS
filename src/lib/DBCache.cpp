/*
 * To change this license header, choose License Headers in Project Properties.
 * To change this template file, choose Tools | Templates
 * and open the template in the editor.
 */

/* 
 * File:   DBCache.cpp
 * Author: thomas
 * 
 * Created on 5. Januar 2016, 17:17
 */

#include <annis/DBCache.h>
#include <annis/db.h>

#include <humblelogging/api.h>

HUMBLE_LOGGER(logger, "default");

using namespace annis;

extern "C" size_t getCurrentRSS( );
extern "C" size_t getCurrentVirtualMemory( );

DBCache::DBCache(size_t maxSizeBytes)
: loadedDBSizeTotal(0), maxLoadedDBSize(maxSizeBytes) {
}

std::shared_ptr<DB> DBCache::initDB(const DBCacheKey& key) {
  std::shared_ptr<DB> result = std::make_shared<DB>();

  auto oldProcessMemory = getCurrentRSS();
  bool loaded = result->load(key.corpusPath);
  if (!loaded) {
    std::cerr << "FATAL ERROR: coult not load corpus from " << key.corpusPath << std::endl;
    std::cerr << "" << __FILE__ << ":" << __LINE__ << std::endl;
    exit(-1);
  }

  if (key.forceFallback) {
    // manually convert all components to fallback implementation
    auto components = result->getAllComponents();
    for (auto c : components) {
      result->convertComponent(c, GraphStorageRegistry::fallback);
    }
  } else {
    result->optimizeAll(key.overrideImpl);
  }

  auto newProcessMemory = getCurrentRSS();

  size_t loadedSize = 1L;
  if(newProcessMemory >  oldProcessMemory)
  {
    loadedSize = newProcessMemory - oldProcessMemory;
  }
  else
  {
    HL_WARN(logger, "Invalid size for new corpus");
  }
  loadedDBSize[key] = loadedSize;
  loadedDBSizeTotal += loadedSize;
  
  return result;
}

DBCache::~DBCache() {
}

